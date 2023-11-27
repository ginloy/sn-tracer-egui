use sn_tracer_egui::App;
fn main() {
    env_logger::Builder::from_default_env().init();
    let options = eframe::NativeOptions::default();
    eframe::run_native("test", options, Box::new(|cc| Box::new(App::new(cc))))
        .expect("Failed to launch");
}


