use std::{
    path::PathBuf,
    process::Stdio,
};

use anyhow::{bail, Context, Result};
use eframe::egui;
use itertools::Itertools;
use log::*;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc::UnboundedSender,
    time::{interval, timeout, Duration},
};
use tokio_serial::{SerialPort, SerialStream};

use crate::HEADERS;

const ERROR: &str = "Channel closed";
const TIMEOUT_MS: u64 = 2000;
pub enum Command {
    Connect,
    Write(String),
    Read,
    Download(PathBuf, Vec<String>, Vec<String>),
    Terminate,
}

pub enum Reply {
    Connected(String),
    Connecting,
    Read(String),
    ReadError(String),
    WriteError(String),
    Keypress(std::time::SystemTime, String),
    Disconnected,
    DownloadError(String),
    BarcodeOutput(String),
}

fn get_available_devices() -> Vec<String> {
    let devices = tokio_serial::available_ports().unwrap_or(Vec::new());
    devices
        .into_iter()
        .filter(|d| {
            if let tokio_serial::SerialPortType::UsbPort(ref info) = d.port_type {
                debug!("Detected: {}, {:?}", d.port_name, info);
                return info.pid == 24577;
            }
            false
        })
        .map(|d| d.port_name)
        .collect()
}

async fn autoconnect(send_channel: &UnboundedSender<Reply>) -> Result<BufReader<SerialStream>> {
    let devices = get_available_devices();
    if !devices.is_empty() {
        send_channel.send(Reply::Connecting).expect(ERROR);
    }
    let mut futures = tokio::task::JoinSet::new();
    devices.into_iter().for_each(|d| {
        futures.spawn(try_connect(d));
    });
    debug!("Connection attempts: {}", futures.len());
    let mut results = Vec::new();
    while let Some(Ok(res)) = futures.join_next().await {
        results.push(res);
    }
    results
        .into_iter()
        .find_map(Result::ok)
        .context("Failed to connect to available devices")
}

async fn try_connect(device: String) -> Result<BufReader<SerialStream>> {
    let handle = SerialStream::open(&tokio_serial::new(&device, 9600))?;
    let mut handle = BufReader::new(handle);
    debug!("Handle obtained: {:?}", handle);
    tokio::time::sleep(Duration::from_millis(TIMEOUT_MS)).await;
    for _ in 0..5 {
        handle.write_all("connect\n".as_bytes()).await?;
        let reply = read_line_timeout(&mut handle).await?;
        debug!("Reply: {}", reply);
        if reply.trim() == "connected" {
            return Ok(handle);
        }
    }
    bail!("Could not establish handshake with {:?}", handle)
}

async fn listen(
    channel: tokio::sync::mpsc::UnboundedSender<Reply>,
    ctx: egui::Context,
) -> Result<()> {
    let scanner_path = std::env::var("SCANNER_PATH")?;
    let mut scanner = tokio::process::Command::new(scanner_path)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .spawn()?;
    let output = scanner
        .stdout
        .take()
        .context("Failed to get stdout of scanner")?;
    let mut output = BufReader::new(output);

    let mut buf = String::new();
    loop {
        output.read_line(&mut buf).await?;
        channel.send(Reply::BarcodeOutput(buf.trim().to_string()))?;
        ctx.request_repaint();
        buf.clear();
    }
}

async fn refresh_ui(ctx: egui::Context) {
    let mut interval = interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        ctx.request_repaint();
    }
}

async fn read_line_timeout(handle: &mut BufReader<SerialStream>) -> Result<String> {
    let mut buf = String::new();
    match timeout(
        Duration::from_millis(TIMEOUT_MS),
        handle.read_line(&mut buf),
    )
    .await
    {
        Err(e) => {
            debug!("Timeout: {:?}", e);
            Err(e).with_context(|| "Connection timeout")
        }
        Ok(res) => {
            res.map(|_| ()).map_err(anyhow::Error::from)?;
            Ok(buf)
        }
    }
}

#[tokio::main]
pub async fn start_service(
    mut receive_channel: tokio::sync::mpsc::UnboundedReceiver<Command>,
    send_channel: tokio::sync::mpsc::UnboundedSender<Reply>,
    ctx: egui::Context,
) {
    // {
    //     let channel = send_channel.clone();
    //     let ctx = ctx.clone();
    //     spawn_blocking(move || listen(channel, ctx));
    // }
    tokio::task::spawn({
        let channel = send_channel.clone();
        let ctx = ctx.clone();
        async move {
            if let Err(e) = listen(channel, ctx).await {
                debug!("Could not spawn scanner process: {:?}", e);
            }
        }
    });
    tokio::spawn({
        let ctx = ctx.clone();
        async move { refresh_ui(ctx).await }
    });
    let mut handle: Option<BufReader<SerialStream>> = None;

    loop {
        match receive_channel.recv().await {
            Some(Command::Connect) => {
                debug!("Connection request");
                handle = match autoconnect(&send_channel).await {
                    Ok(handle) => {
                        send_channel
                            .send(Reply::Connected(
                                handle.get_ref().name().unwrap_or("Unknown port".into()),
                            ))
                            .expect(ERROR);
                        Some(handle)
                    }
                    Err(e) => {
                        debug!("Connection error: {:?}", e);
                        send_channel.send(Reply::Disconnected).expect(ERROR);
                        None
                    }
                };
            }
            Some(Command::Write(s)) => {
                handle = match handle {
                    None => {
                        send_channel.send(Reply::WriteError("Not connected".into()));
                        send_channel.send(Reply::Disconnected).expect(ERROR);
                        None
                    }
                    Some(mut handle) => match handle.write_all(s.as_bytes()).await {
                        Err(e) => {
                            send_channel.send(Reply::WriteError(format!("{:?}", e)));
                            send_channel.send(Reply::Disconnected).expect(ERROR);
                            None
                        }
                        Ok(_) => {
                            handle.flush().await;
                            Some(handle)
                        }
                    },
                }
            }
            Some(Command::Read) => {
                handle = match handle {
                    None => {
                        send_channel
                            .send(Reply::ReadError("Not connected".into()))
                            .expect(ERROR);
                        send_channel.send(Reply::Disconnected).expect(ERROR);
                        None
                    }
                    Some(mut handle) => {
                        let result = read_line_timeout(&mut handle).await.context("Read error");
                        match result {
                            Err(e) => {
                                send_channel
                                    .send(Reply::ReadError(format!("{:?}", e)))
                                    .expect(ERROR);
                                send_channel.send(Reply::Disconnected).expect(ERROR);
                                None
                            }
                            Ok(s) => {
                                send_channel.send(Reply::Read(s)).expect(ERROR);
                                Some(handle)
                            }
                        }
                    }
                }
            }
            Some(Command::Download(path, barcode, device)) => {
                debug!("Download to {:?}", path);
                let mut data = HEADERS.join(",");
                data.push('\n');
                let rows = barcode.len().max(device.len());
                data.push_str(
                    &(0..rows)
                        .map(|i| {
                            format!(
                                "{},{}",
                                barcode.get(i).unwrap_or(&"".to_string()),
                                device.get(i).unwrap_or(&"".to_string())
                            )
                        })
                        .join("\n"),
                );
                if let Err(e) = std::fs::write(path, data.as_bytes()) {
                    send_channel
                        .send(Reply::DownloadError(
                            format!("Download failed: {:?}", e).into(),
                        ))
                        .expect(ERROR);
                }
            }
            Some(Command::Terminate) => {
                break;
            }
            None => break,
        }
        ctx.request_repaint();
    }
}
