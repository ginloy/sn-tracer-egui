use anyhow::{anyhow, Context, Result};
use eframe::egui;
use log::debug;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    select,
    task::spawn_blocking,
    time::{interval, timeout, Duration},
};
use tokio_serial::{SerialPort, SerialStream};

const ERROR: &str = "Channel closed";
const TIMEOUT_MS: u64 = 500;
pub enum Command {
    Connect,
    Write(String),
    Read,
}

pub enum Reply {
    Connected(String),
    Read(String),
    Keypress(std::time::SystemTime, String),
    Disconnected,
}

fn get_available_devices() -> Vec<String> {
    let devices = tokio_serial::available_ports().unwrap_or(Vec::new());
    devices
        .into_iter()
        .filter(|d| {
            if let tokio_serial::SerialPortType::UsbPort(ref info) = d.port_type {
                return info.manufacturer.as_ref().map(|s| s.as_str()).unwrap_or("")
                    == "Arduino (www.arduino.cc)";
            }
            false
        })
        .map(|d| d.port_name)
        .collect()
}

async fn autoconnect() -> Option<BufWriter<BufReader<SerialStream>>> {
    let devices = get_available_devices();
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

fn listen(channel: std::sync::mpsc::Sender<Reply>, ctx: egui::Context) {
    rdev::listen(move |event| match event.name {
        Some(s) => {
            ctx.request_repaint();
            channel.send(Reply::Keypress(event.time, s)).unwrap();
        }
        None => {}
    })
    .unwrap();
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

pub async fn start_service(
    receive_channel: std::sync::mpsc::Receiver<Command>,
    send_channel: std::sync::mpsc::Sender<Reply>,
    ctx: egui::Context,
) {
    {
        let channel = send_channel.clone();
        spawn_blocking(|| listen(channel, ctx));
    }
    let mut interval = interval(Duration::from_millis(10));
    let mut handle: Option<BufWriter<BufReader<SerialStream>>> = None;

    loop {
        interval.tick().await;
        match receive_channel.try_recv() {
            Ok(Command::Connect) => {
                handle = autoconnect().await;
                if let Some(ref handle) = handle {
                    send_channel
                        .send(Reply::Connected(
                            handle.get_ref().get_ref().name().unwrap_or("".to_string()),
                        ))
                        .expect(ERROR);
                }
            }
            Ok(Command::Write(s)) => {
                handle = match handle {
                    None => {
                        send_channel.send(Reply::Disconnected).expect(ERROR);
                        None
                    }
                    Some(mut handle) => match handle.write_all(s.as_bytes()).await {
                        Err(e) => {
                            debug!("{:?}", e);
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
                        send_channel.send(Reply::Disconnected).expect(ERROR);
                        None
                    }
                    Some(mut handle) => {
                        let result = read_line_timeout(&mut handle).await.context("Read error");
                        match result {
                            Err(e) => {
                                debug!("{:?}", e);
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
            Err(_) => (),
        }
    }
}
