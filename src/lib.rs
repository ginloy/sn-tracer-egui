use eframe::egui;
use log::*;
use service::{Command, Reply};
use std::{
    sync::mpsc::{Receiver, Sender},
    time::SystemTime,
};
use itertools::Itertools;

mod service;

pub struct App {
    barcode_input: Vec<String>,
    device_output: Vec<String>,
    receive_channel: Receiver<Reply>,
    send_channel: Sender<Command>,
    keypress_buffer: Vec<(SystemTime, String)>,
    connection_status: ConnectionStatus,
}

enum ConnectionStatus {
    Connected(String),
    Connecting,
    Disconnected,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let (send_channel_1, receive_channel_1) = std::sync::mpsc::channel();
        let (send_channel_2, receive_channel_2) = std::sync::mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn({
            let ctx = ctx.clone();
            move || service::start_service(receive_channel_1, send_channel_2, ctx.clone())
        });
        Self {
            barcode_input: Vec::new(),
            device_output: Vec::new(),
            receive_channel: receive_channel_2,
            send_channel: send_channel_1,
            keypress_buffer: Vec::new(),
            connection_status: ConnectionStatus::Disconnected,
        }
    }

    fn add_keypress(&mut self, time: SystemTime, s: String) {
        if self.keypress_buffer.is_empty() {
            self.keypress_buffer.push((time, s));
        } else {
            let (last_time, _) = self.keypress_buffer.last().unwrap();
            if time
                .duration_since(last_time.clone())
                .unwrap_or(std::time::Duration::from_secs(1))
                < std::time::Duration::from_millis(10)
            {
                if &s == "\r" {
                    self.barcode_input.push(
                        self.keypress_buffer
                            .iter()
                            .map(|(_, s)| s)
                            .cloned()
                            .collect::<String>(),
                    );
                } else {
                    self.keypress_buffer.push((time, s));
                }
            } else {
                self.keypress_buffer.clear();
                self.keypress_buffer.push((time, s));
            }
        }
    }

    fn flush_receive_channel(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.receive_channel.try_recv() {
            match event {
                Reply::Keypress(e, s) => {
                    self.add_keypress(e, s);
                }
                Reply::Read(s) => {
                    self.device_output.push(s.trim().to_string());
                }
                Reply::Connected(d) => {
                    self.connection_status = ConnectionStatus::Connected(d);
                }
                Reply::Disconnected => {
                    self.connection_status = ConnectionStatus::Disconnected;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            debug!("running");
            self.flush_receive_channel(ctx);
            ui.label(self.barcode_input.iter().join("\n"))
        });
    }
}
