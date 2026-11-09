mod app;
mod curve_math;
mod tabs;
pub mod theme;

use app::X56App;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([960.0, 680.0])
            .with_title("X-56 RHINO // RUST-EVDEV Control Center"),
        ..Default::default()
    };

    eframe::run_native(
        "x56_gui",
        options,
        Box::new(|cc| {
            theme::configure_theme(&cc.egui_ctx);
            Ok(Box::new(X56App::default()))
        }),
    )
}
