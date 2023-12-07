use egui::*;
use itertools::Itertools;
use log::*;
use rfd::*;
use service::{Command, Reply};
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

mod service;
mod widgets {
    pub mod csv_table;
}

use widgets::csv_table::CsvTable;

const HEADERS: [&str; 4] = [
    "Barcode",
    "Serial Number (HEX)",
    "Serial Number (DEC)",
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
    keyboard: bool,
    is_scanner_alive: bool,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
struct AppStorage {
    barcode_input: Vec<String>,
    text: String,
    device_output: Vec<String>,
    keypress_buffer: Vec<(SystemTime, String)>,
    download_path: Option<PathBuf>,

    #[serde(default = "default_keyboard")]
    keyboard: bool,
}

fn default_keyboard() -> bool {
    false
}

impl AppStorage {
    fn from(app: &App) -> Self {
        Self {
            barcode_input: app.barcode_input.clone(),
            text: app.text.clone(),
            device_output: app.device_output.clone(),
            keypress_buffer: app.keypress_buffer.clone(),
            download_path: app.download_path.clone(),
            keyboard: app.keyboard,
        }
    }

    fn into(
        self,
        receive_channel: UnboundedReceiver<Reply>,
        send_channel: UnboundedSender<Command>,
    ) -> App {
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
            keyboard: self.keyboard,
            is_scanner_alive: true,
        }
    }
}

enum ConnectionStatus {
    Connected(String),
    Connecting,
    Disconnected,
}

fn ask_confirmation(msg: &str) -> bool {
    match MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Warning")
        .set_description(msg)
        .set_buttons(MessageButtons::OkCancel)
        .show()
    {
        MessageDialogResult::Ok => true,
        MessageDialogResult::Cancel => false,
        _ => false,
    }
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
            Some(storage)
                if eframe::get_value::<AppStorage>(storage, eframe::APP_KEY).is_some() =>
            {
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
                keyboard: false,
                is_scanner_alive: true,
            },
        }
    }

    fn update_non_ui(&mut self) {
        match &self.connection_status {
            ConnectionStatus::Disconnected => {
                if self.previous_connection_request.elapsed()
                    > std::time::Duration::from_millis(200)
                {
                    self.previous_connection_request = Instant::now();
                    self.send_channel.send(Command::Connect).unwrap();
                }
            }
            ConnectionStatus::Connected(_) => {
                if self.previous_connection_request.elapsed()
                    > std::time::Duration::from_millis(200)
                {
                    self.previous_connection_request = Instant::now();
                    self.send_channel.send(Command::CheckConnection).unwrap();
                }
            }
            _ => {}
        }
    }

    fn get_csv(&self) -> String {
        let rows = std::cmp::max(self.barcode_input.len(), self.device_output.len());
        let mut barcodes = self.barcode_input.iter().map(String::as_str);
        let mut device_output = self.device_output.iter().map(String::as_str);
        let body = (0..rows)
            .map(|_| {
                let barcode = barcodes.next().unwrap_or("");
                let device_output = device_output.next().unwrap_or("");
                format!("{},{}", barcode, device_output)
            })
            .join("\n");
        HEADERS.join(",") + "\n" + &body
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

    fn flush_receive_channel(&mut self, _ctx: &egui::Context) {
        while let Ok(event) = self.receive_channel.try_recv() {
            debug!("Received event: {:?}", event);
            match event {
                Reply::Read(s) => {
                    self.device_output.push(s.trim().into());
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
                    self.send_channel.send(Command::Read).expect("Thread died");
                }
                Reply::ScannerStartFail => {
                    self.is_scanner_alive = false;
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
        Duration::from_millis(500)
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ctx.request_repaint_after(Duration::from_secs(2));
        self.update_non_ui();
        self.flush_receive_channel(ctx);
        egui::TopBottomPanel::top("top_panel")
            .exact_height(50.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.heading(match &self.connection_status {
                        ConnectionStatus::Connected(_) => "Connected".into(),
                        ConnectionStatus::Connecting => "Attempting Connection...".to_string(),
                        ConnectionStatus::Disconnected => "Disconnected".to_string(),
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let clear_button =
                            Button::new(RichText::new("Clear").heading()).fill(Color32::RED);
                        if ui.add(clear_button).clicked()
                            && ask_confirmation("Are you sure you want to clear all data?")
                        {
                            self.barcode_input.clear();
                            self.device_output.clear();
                        };
                        let download_bytton =
                            Button::new(RichText::new("Download").heading()).rounding(5.0);
                        if ui.add(download_bytton).clicked() {
                            self.start_download();
                        };
                        if ui
                            .add(Button::new(RichText::new("⌨").heading()).selected(self.keyboard))
                            .clicked()
                        {
                            self.keyboard = !self.keyboard;
                            if self.keyboard {
                                self.send_channel
                                    .send(Command::StopScanner)
                                    .expect("Thread died");
                                self.is_scanner_alive = false;
                            } else {
                                self.send_channel
                                    .send(Command::StartScanner)
                                    .expect("Thread died");
                                self.is_scanner_alive = true;
                            }
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("bottom_panel")
            .exact_height(40.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if !self.keyboard {
                        if self.is_scanner_alive {
                            ui.label("Scanning barcodes...");
                        } else {
                            ui.add(Label::new(
                                RichText::new("Barcode scanner task failed to start")
                                    .color(Color32::RED),
                            ));
                        }
                        return;
                    }
                    let input_box = ui.add(
                        egui::TextEdit::singleline(&mut self.text)
                            .desired_width(ui.available_width()),
                    );
                    // input_box.request_focus();
                    if input_box.ctx.input(|i| i.key_pressed(egui::Key::Enter))
                        && !self.text.trim().is_empty()
                    {
                        self.barcode_input.push(self.text.clone());
                        self.send_channel.send(Command::Read).expect("Thread died");
                        self.text.clear();
                        input_box.request_focus();
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(CsvTable::new(&self.get_csv()).expect("This should not happen"));
        });
    }
}
