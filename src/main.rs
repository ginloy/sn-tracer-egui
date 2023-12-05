#![cfg_attr(all(target_os = "windows", not(feature = "console")), windows_subsystem = "windows")]
use sn_tracer_egui::App;


fn main() {
    env_logger::Builder::from_default_env().init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "sn-tracer",
        options,
        Box::new(move |cc: &eframe::CreationContext| Box::new(App::new(&cc))),
    )
    .expect("Failed to launch");
}
