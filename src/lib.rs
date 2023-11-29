use eframe::{
    egui,
    epaint::{FontFamily, Vec2},
};
use egui::*;
use egui_extras::*;
use itertools::Itertools;
use log::*;
use service::{Command, Reply};
use std::{
    sync::mpsc::{Receiver, Sender},
    time::SystemTime,
};

mod service;

pub struct App {
    barcode_input: Vec<String>,
    text: String,
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
    fn configure_text_styles(ctx: &egui::Context) {
        use FontFamily::Proportional;
        use TextStyle::*;

        let mut style = (*ctx.style()).clone();
        style.text_styles = [
            (Heading, FontId::new(30.0, Proportional)),
            (Body, FontId::new(18.0, Proportional)),
            (Monospace, FontId::new(14.0, Proportional)),
            (Button, FontId::new(14.0, Proportional)),
            (Small, FontId::new(10.0, Proportional)),
        ]
        .into();
        ctx.set_style(style);
    }
    pub fn new(cc: &eframe::CreationContext) -> Self {
        // Self::configure_text_styles(&cc.egui_ctx);
        let (send_channel_1, receive_channel_1) = std::sync::mpsc::channel();
        let (send_channel_2, receive_channel_2) = std::sync::mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn({
            let ctx = ctx.clone();
            move || service::start_service(receive_channel_1, send_channel_2, ctx.clone())
        });
        Self {
            barcode_input: Vec::new(),
            text: String::new(),
            device_output: Vec::new(),
            receive_channel: receive_channel_2,
            send_channel: send_channel_1,
            keypress_buffer: Vec::new(),
            connection_status: ConnectionStatus::Disconnected,
        }
    }
    
    fn update_non_ui(&mut self) {
        if let ConnectionStatus::Disconnected = self.connection_status {
            self.send_channel.send(Command::Connect).expect("Thread died");
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
                Reply::Connecting => {
                    self.connection_status = ConnectionStatus::Connecting;
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
        self.update_non_ui();
        egui::TopBottomPanel::bottom("bottom_panel")
            .exact_height(40.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let input_box = ui.add(
                        egui::TextEdit::singleline(&mut self.text)
                            .desired_width(ui.available_width() - 80.0),
                    );
                    input_box.request_focus();
                    if input_box.ctx.input(|i| i.key_pressed(egui::Key::Enter))
                        && !self.text.trim().is_empty()
                    {
                        self.barcode_input.push(self.text.clone());
                        self.send_channel
                            .send(Command::Write("read\n".to_string()))
                            .expect("Thread died");
                        self.send_channel.send(Command::Read).expect("Thread died");
                        self.text.clear();
                    }
                    if ui
                        .add(egui::Button::new("Download").min_size(Vec2 {
                            x: ui.available_width(),
                            y: input_box.rect.height(),
                        }))
                        .clicked()
                    {
                        todo!();
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.flush_receive_channel(ctx);
            ui.heading(match &self.connection_status {
                ConnectionStatus::Connected(d) => format!("Connected to {}", d),
                ConnectionStatus::Connecting => "Attempting Connection...".to_string(),
                ConnectionStatus::Disconnected => "Disconnected".to_string(),
            });
            ui.add_space(20.0);
            TableBuilder::new(ui)
                .stick_to_bottom(true)
                .striped(true)
                .resizable(true)
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::auto())
                .column(Column::auto())
                .column(Column::auto())
                .min_scrolled_height(0.0)
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("Barcode");
                    });
                    header.col(|ui| {
                        ui.strong("Serial Number");
                    });
                })
                .body(|mut body| {
                    body.row(30.0, |mut row| {
                        row.col(|ui| {
                            ui.label("Hello");
                        });
                        row.col(|ui| {
                            ui.label("World");
                        });
                    })
                });
        });
    }
}
