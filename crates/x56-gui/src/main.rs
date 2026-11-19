mod app;
mod curve_math;
pub mod icons;
mod tabs;
pub mod theme;
pub mod tray;

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

    let socket_path = tray::default_instance_socket_path();
    let (mut instance_listener, _socket_cleanup) = match tray::acquire_single_instance(&socket_path)
    {
        Ok(tray::SingleInstanceGuard::Secondary) => {
            log::debug!("Instancia secundaria detectada; enfocando ventana existente");
            return Ok(());
        }
        Ok(tray::SingleInstanceGuard::Primary { listener, path }) => {
            (Some(listener), Some(tray::SocketCleanup(path)))
        }
        Err(err) => {
            log::warn!("No se pudo inicializar el socket de instancia única: {err}");
            (None, None)
        }
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([720.0, 520.0])
        .with_title("X56 HOTAS Deck // Control Center");

    if let Some(icon) = tray::load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "x56_gui",
        options,
        Box::new(move |cc| {
            theme::configure_theme(&cc.egui_ctx);
            Ok(Box::new(X56App::new(
                &cc.egui_ctx,
                instance_listener.take(),
            )))
        }),
    )
}
