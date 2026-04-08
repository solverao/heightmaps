mod app;
mod types;
mod ui;
mod view3d;

use app::HeightmapApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([640.0, 480.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Heightmap Generator",
        options,
        Box::new(|_cc| Ok(Box::new(HeightmapApp::default()))),
    )
}
