use eframe::egui;
use egui::*;
pub struct TestApp {
    text: String,
}

impl TestApp {
    pub fn new() -> Self {
        Self {
            text: "Hello World!".to_owned(),
        }
    }
}

impl eframe::App for TestApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
            ui.add(Label::new(&self.text));
            ui.horizontal(|ui| {
                ui.label("Text:");
                ui.text_edit_singleline(&mut self.text);
            });
        });
    }
}
