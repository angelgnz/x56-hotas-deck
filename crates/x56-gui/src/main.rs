mod app;
mod curve_math;
pub mod icons;
mod tabs;
pub mod theme;

use app::X56App;
use eframe::egui;

fn init_logging() {
    let debug_flag = std::env::args().any(|arg| arg == "--debug" || arg == "-d");
    let rust_log_set = std::env::var_os("RUST_LOG").is_some();

    if debug_flag || rust_log_set {
        let mut builder = env_logger::Builder::from_default_env();

        if debug_flag && !rust_log_set {
            builder.filter_level(log::LevelFilter::Debug);
        }

        builder.format_timestamp_millis();
        builder.format_module_path(true);
        builder.format_target(true);

        if let Err(err) = builder.try_init() {
            eprintln!("Aviso: no se pudo inicializar env_logger: {err}");
        } else {
            log::debug!(
                "Logger inicializado (debug_flag={}, RUST_LOG={:?})",
                debug_flag,
                std::env::var("RUST_LOG").ok()
            );
        }
    }
}

fn main() -> eframe::Result<()> {
    init_logging();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([720.0, 520.0])
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
