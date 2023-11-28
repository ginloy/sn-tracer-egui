use eframe::egui;
use sn_tracer_egui::App;
use std::sync::mpsc::{Receiver, Sender};
fn main() {
    env_logger::Builder::from_default_env().init();
    let mut options = eframe::NativeOptions::default();
    // options.viewport = options.viewport.with_active(true).with_always_on_top().with_mouse_passthrough(true).with_transparent(true);
    eframe::run_native("test", options, Box::new(move |cc: &eframe::CreationContext| {
        Box::new(App::new(&cc))
    }))
        .expect("Failed to launch");
}


