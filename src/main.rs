use eframe::egui;
use sn_tracer_egui::App;
use std::sync::mpsc::{Receiver, Sender};
fn main() {
    env_logger::Builder::from_default_env().init();
    let options = eframe::NativeOptions::default();
    eframe::run_native("test", options, Box::new(move |cc: &eframe::CreationContext| {
        Box::new(App::new(&cc))
    }))
        .expect("Failed to launch");
}


