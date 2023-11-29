use std::{path::PathBuf, sync::mpsc::Sender, process::{Stdio, self}, io::BufRead};

use anyhow::{anyhow, Context, Result};
use eframe::egui;
use itertools::Itertools;
use log::{debug, trace};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    task::spawn_blocking,
    time::{interval, timeout, Duration},
};
use tokio_serial::{SerialPort, SerialStream};

use crate::HEADERS;

const ERROR: &str = "Channel closed";
const TIMEOUT_MS: u64 = 500;
pub enum Command {
    Connect,
    Write(String),
    Read,
    Download(PathBuf, Vec<String>, Vec<String>),
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
    BarcodeOutput(String)
}

fn get_available_devices() -> Vec<String> {
    let devices = tokio_serial::available_ports().unwrap_or(Vec::new());
    devices
        .into_iter()
        .filter(|d| {
            if let tokio_serial::SerialPortType::UsbPort(ref info) = d.port_type {
                debug!("Detected: {}, {:?}", d.port_name, info);
                return info.manufacturer.as_ref().map(|s| s.as_str()).unwrap_or("")
                    == "Arduino (www.arduino.cc)";
            }
            false
        })
        .map(|d| d.port_name)
        .collect()
}

async fn autoconnect(send_channel: &Sender<Reply>) -> Option<BufWriter<BufReader<SerialStream>>> {
    let devices = get_available_devices();
    if !devices.is_empty() {
        send_channel.send(Reply::Connecting).expect(ERROR);
    }
    let mut futures = tokio::task::JoinSet::new();
    devices.into_iter().for_each(|d| {
        futures.spawn(try_connect(d));
    });
    futures
        .join_next()
        .await
        .map(|r| r.ok())
        .flatten()
        .flatten()
}

async fn try_connect(device: String) -> Option<BufWriter<BufReader<SerialStream>>> {
    let handle = SerialStream::open(&tokio_serial::new(&device, 9600)).ok()?;
    let mut handle = BufWriter::new(BufReader::new(handle));
    for _ in 0..5 {
        handle.write_all("connect\n".as_bytes()).await.ok()?;
        let reply = read_line_timeout(&mut handle).await.ok()?;
        if reply.trim() == "connected" {
            return Some(handle);
        }
    }
    None
}

fn listen(channel: std::sync::mpsc::Sender<Reply>, ctx: egui::Context) -> Result<()> {
    let mut scanner = process::Command::new("./target/release/scanner")
    .stdout(Stdio::piped())
    .spawn()?;
    let output = scanner.stdout.take().context("Failed to get stdout of scanner")?;
    let mut output = std::io::BufReader::new(output);

    let mut buf = String::new();
    loop {
        output.read_line(&mut buf)?;
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

async fn read_line_timeout(handle: &mut BufWriter<BufReader<SerialStream>>) -> Result<String> {
    let mut buf = String::new();
    match timeout(
        Duration::from_millis(TIMEOUT_MS),
        handle.read_line(&mut buf),
    )
    .await
    {
        Err(_) => Err(anyhow!("Timeout")),
        Ok(res) => {
            res.map(|_| ()).map_err(|_| anyhow!("Read error"))?;
            Ok(buf)
        }
    }
}

#[tokio::main]
pub async fn start_service(
    receive_channel: std::sync::mpsc::Receiver<Command>,
    send_channel: std::sync::mpsc::Sender<Reply>,
    ctx: egui::Context,
) {
    // {
    //     let channel = send_channel.clone();
    //     let ctx = ctx.clone();
    //     spawn_blocking(move || listen(channel, ctx));
    // }
    tokio::task::spawn_blocking({
        let channel = send_channel.clone();
        let ctx = ctx.clone();
        move || listen(channel, ctx)
    });
    tokio::spawn({
        let ctx = ctx.clone();
        async move { refresh_ui(ctx).await }
    });
    let mut interval = interval(Duration::from_millis(10));
    let mut handle: Option<BufWriter<BufReader<SerialStream>>> = None;

    loop {
        interval.tick().await;
        match receive_channel.recv() {
            Ok(Command::Connect) => {
                debug!("Connection request");
                handle = autoconnect(&send_channel).await;
                if let Some(ref handle) = handle {
                    send_channel
                        .send(Reply::Connected(
                            handle.get_ref().get_ref().name().unwrap_or("".to_string()),
                        ))
                        .expect(ERROR);
                } else {
                    send_channel.send(Reply::Disconnected).expect(ERROR);
                }
            }
            Ok(Command::Write(s)) => {
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
                        Ok(_) => Some(handle),
                    },
                }
            }
            Ok(Command::Read) => {
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
            Ok(Command::Download(path, barcode, device)) => {
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
            Err(_) => (),
        }
        ctx.request_repaint();
    }
}
