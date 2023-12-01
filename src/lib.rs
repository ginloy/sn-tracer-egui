use eframe::{
    egui,
    // epaint::FontFamily,
};
use egui::*;
use egui_extras::*;
use log::*;
use rfd::*;
use service::{Command, Reply};
use std::{
    path::PathBuf,
    time::{Instant, SystemTime, Duration},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

mod service;

const HEADERS: [&str; 4] = [
    "Barcode",
    "Serial Number (DEC)",
    "Serial Number (HEX)",
    "Manufacture Date",
];

const DEFAULT_SAVE_FILE: &str = "record.csv";

pub struct App {
    barcode_input: Vec<String>,
    text: String,
    device_output: Vec<String>,
    receive_channel: UnboundedReceiver<Reply>,
    send_channel: UnboundedSender<Command>,
    keypress_buffer: Vec<(SystemTime, String)>,
    connection_status: ConnectionStatus,
    download_path: Option<PathBuf>,
    previous_connection_request: Instant,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct AppStorage {
    barcode_input: Vec<String>,
    text:String,
    device_output: Vec<String>,
    keypress_buffer: Vec<(SystemTime, String)>,
    download_path: Option<PathBuf>,
}

impl AppStorage {
    fn from(app: &App) -> Self {
        Self {
            barcode_input: app.barcode_input.clone(),
            text: app.text.clone(),
            device_output: app.device_output.clone(),
            keypress_buffer: app.keypress_buffer.clone(),
            download_path: app.download_path.clone(),
        }
    }
    
    fn into(self, receive_channel: UnboundedReceiver<Reply>, send_channel: UnboundedSender<Command>) -> App {
        App {
            barcode_input: self.barcode_input,
            text: self.text,
            device_output: self.device_output,
            receive_channel,
            send_channel,
            keypress_buffer: self.keypress_buffer,
            connection_status: ConnectionStatus::Disconnected,
            download_path: self.download_path,
            previous_connection_request: Instant::now(),
        }
    }
}


enum ConnectionStatus {
    Connected(String),
    Connecting,
    Disconnected,
}


impl App {
    // fn configure_text_styles(ctx: &egui::Context) {
    //     use FontFamily::Proportional;
    //     use TextStyle::*;

    //     let mut style = (*ctx.style()).clone();
    //     style.text_styles = [
    //         (Heading, FontId::new(30.0, Proportional)),
    //         (Body, FontId::new(18.0, Proportional)),
    //         (Monospace, FontId::new(14.0, Proportional)),
    //         (Button, FontId::new(14.0, Proportional)),
    //         (Small, FontId::new(10.0, Proportional)),
    //     ]
    //     .into();
    //     ctx.set_style(style);
    // }

    pub fn new(cc: &eframe::CreationContext) -> Self {
        // Self::configure_text_styles(&cc.egui_ctx);
        let (send_channel_1, receive_channel_1) = tokio::sync::mpsc::unbounded_channel();
        let (send_channel_2, receive_channel_2) = tokio::sync::mpsc::unbounded_channel();
        let ctx = cc.egui_ctx.clone();
        let _ = std::thread::spawn({
            let ctx = ctx.clone();
            move || service::start_service(receive_channel_1, send_channel_2, ctx.clone())
        });
        send_channel_1.send(Command::Connect).expect("Thread died");
        match cc.storage {
            Some(storage) if eframe::get_value::<AppStorage>(storage, eframe::APP_KEY).is_some() => {
                let app_storage: AppStorage = eframe::get_value(storage, eframe::APP_KEY).unwrap();
                app_storage.into(receive_channel_2, send_channel_1)
            }
            _ => Self {
                barcode_input: Vec::new(),
                text: String::new(),
                device_output: Vec::new(),
                receive_channel: receive_channel_2,
                send_channel: send_channel_1,
                keypress_buffer: Vec::new(),
                connection_status: ConnectionStatus::Disconnected,
                download_path: None,
                previous_connection_request: Instant::now(),
            },
        }
    }

    fn update_non_ui(&mut self) {
        match &self.connection_status {
            ConnectionStatus::Disconnected => {
                if self.previous_connection_request.elapsed() > std::time::Duration::from_secs(2) {
                    self.previous_connection_request = Instant::now();
                    self.send_channel.send(Command::Connect).unwrap();
                }
            }
            _ => {}
        }
    }

    fn show_download_error_dialog(&self, msg: &str) {
        MessageDialog::new()
            .set_level(MessageLevel::Error)
            .set_title("Download failed")
            .set_description(msg)
            .set_buttons(MessageButtons::Ok)
            .show();
    }

    fn get_download_path(&self) -> Option<PathBuf> {
        let current_path = self
            .download_path
            .clone()
            .or_else(|| dirs::download_dir().map(|p| p.join(DEFAULT_SAVE_FILE)))
            .or_else(|| std::env::current_dir().ok())?;
        let filename = current_path.file_name()?.to_str()?;
        let dir = current_path.parent()?;
        FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_directory(dir)
            .set_file_name(filename)
            .save_file()
    }

    fn start_download(&mut self) {
        match self.get_download_path() {
            None => {}
            Some(path) => {
                self.download_path = Some(path.clone());
                debug!("Path set to {:?}, starting download", self.download_path);
                self.send_channel
                    .send(Command::Download(
                        self.download_path.as_ref().unwrap().to_owned(),
                        self.barcode_input.clone(),
                        self.device_output.clone(),
                    ))
                    .expect("Thread died");
            }
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

    fn flush_receive_channel(&mut self, _ctx: &egui::Context) {
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
                Reply::WriteError(s) => {
                    debug!("Write error: {}", s);
                }
                Reply::ReadError(s) => {
                    debug!("Read error: {}", s);
                    self.device_output.push(s.trim().into());
                }
                Reply::DownloadError(e) => {
                    debug!("Download error: {}", e);
                    self.show_download_error_dialog(&e);
                }
                Reply::BarcodeOutput(s) => {
                    self.barcode_input.push(s);
                    self.send_channel
                        .send(Command::Write("read\n".to_string()))
                        .expect("Thread died");
                    self.send_channel.send(Command::Read).expect("Thread died");
                }
            }
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &AppStorage::from(self));
    }
    
    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(2)
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_non_ui();
        self.flush_receive_channel(ctx);
        egui::TopBottomPanel::top("top_panel")
            .exact_height(50.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.heading(match &self.connection_status {
                        ConnectionStatus::Connected(d) => format!("Connected to {}", d),
                        ConnectionStatus::Connecting => "Attempting Connection...".to_string(),
                        ConnectionStatus::Disconnected => "Disconnected".to_string(),
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let clear_button =
                            Button::new(RichText::new("Clear").heading()).fill(Color32::RED);
                        if ui.add(clear_button).clicked() {
                            self.barcode_input.clear();
                            self.device_output.clear();
                        };
                        let download_bytton =
                            Button::new(RichText::new("Download").heading()).rounding(5.0);
                        if ui.add(download_bytton).clicked() {
                            self.start_download();
                        };
                    });
                });
            });
        egui::TopBottomPanel::bottom("bottom_panel")
            .exact_height(40.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let input_box = ui.add(
                        egui::TextEdit::singleline(&mut self.text)
                            .desired_width(ui.available_width()),
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
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
                let width = ui.available_width();
                TableBuilder::new(ui)
                    .stick_to_bottom(true)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(Layout::left_to_right(Align::Center))
                    .cell_layout(Layout::top_down(Align::LEFT))
                    .columns(
                        Column::initial(width / 4.2)
                            .clip(true)
                            .at_least(width / HEADERS.len() as f32 / 2.0),
                        3,
                    )
                    .column(
                        Column::remainder()
                            .at_least(width / HEADERS.len() as f32 / 2.0)
                            .clip(true),
                    )
                    // .min_scrolled_height(0.0)
                    .header(
                        ctx.fonts(|f| f.row_height(&TextStyle::Heading.resolve(&ctx.style()))),
                        |mut header| {
                            for header_name in HEADERS.into_iter() {
                                header.col(|ui| {
                                    ui.add(
                                        Label::new(RichText::new(header_name).strong()).wrap(false),
                                    );
                                });
                            }
                        },
                    )
                    .body(|body| {
                        body.rows(
                            ctx.fonts(|f| f.row_height(&TextStyle::Body.resolve(&ctx.style()))),
                            self.barcode_input.len(),
                            |i, mut row| {
                                row.col(|ui| {
                                    ui.add(Label::new(&self.barcode_input[i]).wrap(false));
                                });
                                let mut cols = self
                                    .device_output
                                    .get(i)
                                    .map(|s| s.as_str())
                                    .unwrap_or("")
                                    .split(",");
                                for _ in 0..HEADERS.len() - 1 {
                                    row.col(|ui| {
                                        ui.label(cols.next().unwrap_or("-"));
                                    });
                                }
                            },
                        )
                    });
            })
        });
    }
}
