use std::sync::mpsc::{Receiver, Sender};
use eframe::egui;
use log::*;

mod service;

pub struct App {
    buffer: String,
    receiver: Receiver<rdev::Event>,
    keypress_buffer: Vec<rdev::Event>,
    prev_frame: std::time::Instant
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let _ = std::thread::spawn(move || listen(sender, ctx));
        Self {
            buffer: String::new(),
            receiver,
            keypress_buffer: Vec::new(),
            prev_frame: std::time::Instant::now()
        }
    }
    
    fn flush_receiver(&mut self) {
            while let Ok(event) = self.receiver.try_recv() {
                if self.keypress_buffer.is_empty() {
                    self.keypress_buffer.push(event);
                } else {
                    let last = self.keypress_buffer.last().unwrap();
                    if event.time.duration_since(last.time).unwrap_or(std::time::Duration::from_secs(1))
                        < std::time::Duration::from_millis(10)
                    {
                        if event.name.as_ref().filter(|s| s.as_str() == "\r").is_some()  {
                            self.buffer.push_str(
                                self.keypress_buffer
                                    .iter()
                                    .map(|e| event_to_string(e))
                                    .chain(Some("\n".to_string()).into_iter())
                                    .collect::<String>()
                                    .as_str(),
                            );
                        } else {
                            self.keypress_buffer.push(event);
                        }
                    } else {
                        self.keypress_buffer.clear();
                        self.keypress_buffer.push(event);
                    }
                }
            }

    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            debug!("running");
            self.flush_receiver();
            ui.horizontal(|ui| {
                ui.label(&self.buffer);
                let fps = 1.0 / (std::time::Instant::now() - self.prev_frame).as_secs_f64();
                self.prev_frame = std::time::Instant::now();
                ui.label(format!("FPS: {:.2}", fps));
            });
            ui.label(&self.buffer);
        });
    }
}

fn listen(channel: Sender<rdev::Event>, ctx: egui::Context) {
    if let Err(e) = rdev::listen(move |event: rdev::Event| {
        if event.name.is_none() {
            return;
        }
        debug!("Event detected: {:?}", event);
        channel.send(event).unwrap();
        ctx.request_repaint();
    }) {
        error!("{:?}, exciting...", e);
    }
}

fn event_to_string(event: &rdev::Event) -> String {
    match event.name.as_ref().map(|s| s.as_str()) {
        Some(s) => s.to_string(),
        None => String::new(),
    }
}