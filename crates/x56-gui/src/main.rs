mod app;
mod curve_math;
mod tabs;

use app::X56App;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 720.0])
            .with_min_inner_size([880.0, 600.0])
            .with_title("Saitek / Logitech X56 HOTAS Control Center"),
        ..Default::default()
    };

    eframe::run_native(
        "x56_gui",
        options,
        Box::new(|_cc| Ok(Box::new(X56App::default()))),
    )
}
