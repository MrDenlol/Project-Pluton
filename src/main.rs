#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;

fn main() -> eframe::Result {
    eframe::run_native(
        "Circuit Lab — редактор цифровых схем",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1360., 820.])
                .with_min_inner_size([1100., 700.])
                .with_maximized(true),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ui::CircuitApp::new(cc)))),
    )
}
