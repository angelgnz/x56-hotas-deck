use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use x56_core::{
    all_button_descriptors, axis_name, catalog_for_device, parse_color, ButtonDescriptor,
    ButtonMonitor, Calibrator, CheckStatus, CommandCell, CurveProfile, DeviceKind, Doctor,
    FullProfile, ModeSlot, ProfileManager, ProgrammingProfile, TargetDevice, X56Descriptor,
    X56Manager,
};

#[derive(Parser)]
#[command(
    name = "x56",
    author = "Angel",
    version,
    about = "Herramienta moderna de configuración para Saitek X-56 Rhino y Logitech G X56 HOTAS",
    long_about = "Permite configurar iluminación LED RGB, curvas de respuesta de ejes, zonas muertas y calibración de hardware en Linux mediante hidraw sin desconectar el driver del joystick."
)]
struct Cli {
    /// Activa modo debug y registro detallado de eventos
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Lista los periféricos X56 detectados en la interfaz de configuración (hidraw)
    List {
        /// Salida en formato JSON
        #[arg(long)]
        json: bool,
    },

    /// Configura el color de la iluminación LED RGB
    Rgb(RgbArgs),

    /// Consulta o modifica las curvas de respuesta y zonas muertas de los ejes
    Axis {
        #[command(subcommand)]
        command: AxisCommands,
    },

    /// Exporta o aplica perfiles completos de configuración en formato TOML
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Diagnostica permisos udev, conectividad USB e integridad del sistema
    Doctor {
        /// Salida en formato JSON
        #[arg(long)]
        json: bool,
    },

    /// Calibra el punto cero del sensor Hall (centrado) de la palanca (Stick)
    Calibrate(CalibrateArgs),

    /// Restaura los ajustes de fábrica predeterminados del hardware
    ResetDefaults {
        /// Dispositivo destino: stick, throttle o all (por defecto: all)
        #[arg(short, long, default_value = "all")]
        device: CliTargetDevice,
    },

    /// Catálogo de botones, estado en vivo y asignación de teclas por modo (M1/M2/S1)
    Buttons {
        #[command(subcommand)]
        command: ButtonsCommands,
    },
}

#[derive(Subcommand)]
enum ButtonsCommands {
    /// Lista el catálogo de botones con su ID, sección del manual, código evdev y grupo
    List {
        /// Dispositivo: stick, throttle o all (por defecto: all)
        #[arg(short, long, default_value = "all")]
        device: CliTargetDevice,

        /// Salida en formato JSON
        #[arg(long)]
        json: bool,
    },

    /// Muestra el estado en vivo: modo M1/M2/S1, deslizador SLD y botones pulsados
    Status {
        /// Salida en formato JSON
        #[arg(long)]
        json: bool,
    },

    /// Asigna teclas a un botón en un modo y guarda el perfil TOML
    Bind(ButtonsBindArgs),

    /// Ejecuta el perfil: emite las teclas asignadas por /dev/uinput hasta Ctrl+C
    Run {
        /// Ruta del perfil .toml con las asignaciones
        path: PathBuf,
    },
}

#[derive(Args)]
struct ButtonsBindArgs {
    /// Perfil .toml a modificar (se crea si no existe)
    #[arg(short, long)]
    file: PathBuf,

    /// Modo del selector rotativo: m1, m2 o s1
    #[arg(short, long, ignore_case = true)]
    mode: CliModeSlot,

    /// ID del botón (ver `x56 buttons list`)
    #[arg(short, long)]
    button: String,

    /// Teclas separadas por '+' o ',' (ej: Ctrl+Shift+F5)
    #[arg(short, long)]
    keys: String,

    /// Nombre del comando (por defecto: etiqueta del botón)
    #[arg(short, long)]
    name: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliModeSlot {
    M1,
    M2,
    S1,
}

impl From<CliModeSlot> for ModeSlot {
    fn from(m: CliModeSlot) -> Self {
        match m {
            CliModeSlot::M1 => ModeSlot::M1,
            CliModeSlot::M2 => ModeSlot::M2,
            CliModeSlot::S1 => ModeSlot::S1,
        }
    }
}

#[derive(Args)]
struct RgbArgs {
    /// Color a aplicar: formato hex (#ff8800 o ff8800) o nombre (red, green, blue, white, off, etc.)
    color: String,

    /// Dispositivo destino: stick, throttle o all (por defecto: all)
    #[arg(short, long, default_value = "all")]
    device: CliTargetDevice,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliTargetDevice {
    Stick,
    Throttle,
    All,
}

impl From<CliTargetDevice> for TargetDevice {
    fn from(t: CliTargetDevice) -> Self {
        match t {
            CliTargetDevice::Stick => TargetDevice::Stick,
            CliTargetDevice::Throttle => TargetDevice::Throttle,
            CliTargetDevice::All => TargetDevice::All,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliDeviceKind {
    Stick,
    Throttle,
}

impl From<CliDeviceKind> for DeviceKind {
    fn from(k: CliDeviceKind) -> Self {
        match k {
            CliDeviceKind::Stick => DeviceKind::Stick,
            CliDeviceKind::Throttle => DeviceKind::Throttle,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliCurveProfile {
    S,
    J,
}

impl From<CliCurveProfile> for CurveProfile {
    fn from(p: CliCurveProfile) -> Self {
        match p {
            CliCurveProfile::S => CurveProfile::S,
            CliCurveProfile::J => CurveProfile::J,
        }
    }
}

#[derive(Subcommand)]
enum AxisCommands {
    /// Lee la configuración actual de un eje del dispositivo
    Get {
        /// Dispositivo a consultar: stick o throttle
        #[arg(short, long)]
        device: CliDeviceKind,

        /// Nombre del eje (ej: x, y, z en stick; left, right, r1..r4 en throttle) o ID numérico (ej: 30)
        #[arg(short, long)]
        axis: String,

        /// Salida en formato JSON
        #[arg(long)]
        json: bool,
    },

    /// Modifica la configuración de saturación, zona muerta o curvatura de un eje
    Set(AxisSetArgs),
}

#[derive(Args)]
struct AxisSetArgs {
    /// Dispositivo destino: stick o throttle
    #[arg(short, long)]
    device: CliDeviceKind,

    /// Nombre del eje o ID numérico (ej: x, y, z, 30)
    #[arg(short, long)]
    axis: String,

    /// Saturación X (rango 0 a 1000)
    #[arg(long)]
    xsat: Option<u16>,

    /// Saturación Y (rango 0 a 1000)
    #[arg(long)]
    ysat: Option<u16>,

    /// Zona muerta / Deadband (rango 0 a 1000)
    #[arg(long)]
    deadband: Option<u16>,

    /// Curvatura (rango 0 a 1000, 500 = lineal)
    #[arg(long)]
    curvature: Option<u16>,

    /// Perfil de curva: s (simétrica) o j (asimétrica)
    #[arg(long)]
    profile: Option<CliCurveProfile>,

    /// Salida en formato JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// Guarda el perfil actual de todos los ejes y luces en un archivo TOML
    Save {
        /// Ruta del archivo .toml destino
        path: PathBuf,

        /// Nombre opcional para el perfil
        #[arg(short, long)]
        name: Option<String>,

        /// Color RGB para el Stick (ej: #00ffff)
        #[arg(long)]
        stick_rgb: Option<String>,

        /// Color RGB para el Throttle (ej: #ff5500)
        #[arg(long)]
        throttle_rgb: Option<String>,
    },

    /// Carga y aplica un perfil completo desde un archivo TOML
    Load {
        /// Ruta del archivo .toml a aplicar
        path: PathBuf,

        /// Salida en formato JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
struct CalibrateArgs {
    /// Dispositivo a calibrar (actualmente: stick)
    #[arg(short, long, default_value = "stick")]
    device: CliDeviceKind,

    /// Número de muestras de posición a promediar (por defecto: 50)
    #[arg(short, long, default_value_t = 50)]
    samples: usize,

    /// Solo simular y mostrar valores calculados sin escribir en la EEPROM del dispositivo
    #[arg(long)]
    dry_run: bool,

    /// Salida en formato JSON
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let debug_flag = std::env::args().any(|arg| arg == "--debug" || arg == "-d");
    let rust_log_set = std::env::var_os("RUST_LOG").is_some();
    if debug_flag || rust_log_set {
        let mut builder = env_logger::Builder::from_default_env();
        if debug_flag && !rust_log_set {
            builder.filter_level(log::LevelFilter::Debug);
        }
        builder.format_timestamp_millis();
        builder.format_module_path(true);
        let _ = builder.try_init();
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::List { json } => handle_list(json),
        Commands::Rgb(args) => handle_rgb(args),
        Commands::Axis { command } => match command {
            AxisCommands::Get { device, axis, json } => handle_axis_get(device.into(), &axis, json),
            AxisCommands::Set(args) => handle_axis_set(args),
        },
        Commands::Profile { command } => match command {
            ProfileCommands::Save {
                path,
                name,
                stick_rgb,
                throttle_rgb,
            } => handle_profile_save(path, name, stick_rgb, throttle_rgb),
            ProfileCommands::Load { path, json } => handle_profile_load(path, json),
        },
        Commands::Doctor { json } => handle_doctor(json),
        Commands::Calibrate(args) => handle_calibrate(args),
        Commands::ResetDefaults { device } => handle_reset_defaults(device.into()),
        Commands::Buttons { command } => match command {
            ButtonsCommands::List { device, json } => handle_buttons_list(device, json),
            ButtonsCommands::Status { json } => handle_buttons_status(json),
            ButtonsCommands::Bind(args) => handle_buttons_bind(args),
            ButtonsCommands::Run { path } => handle_buttons_run(&path),
        },
    }
}

fn handle_list(json: bool) -> ExitCode {
    let devices = X56Manager::list_devices();

    if json {
        let json_list: Vec<serde_json::Value> = devices
            .iter()
            .map(|d| {
                serde_json::json!({
                    "vendor": d.vendor.to_string(),
                    "kind": d.kind.to_string(),
                    "vendor_id": format!("{:#06x}", d.vendor_id),
                    "product_id": format!("{:#06x}", d.product_id),
                    "path": d.path.to_string_lossy(),
                    "interface_number": d.interface_number,
                    "product_string": d.product_string,
                    "serial_number": d.serial_number,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_list).unwrap());
        return ExitCode::SUCCESS;
    }

    if devices.is_empty() {
        println!("No se detectaron dispositivos X56 en la interfaz de configuración (Interfaz 2).");
        println!("Verifica que el Stick o Throttle estén conectados y que la regla udev 70-x56.rules esté activa.");
        return ExitCode::FAILURE;
    }

    println!("Dispositivos Saitek / Logitech X56 detectados (Interfaz 2 / hidraw):");
    println!("------------------------------------------------------------------");
    for dev in devices {
        print_device_info(&dev);
    }

    ExitCode::SUCCESS
}

fn print_device_info(d: &X56Descriptor) {
    let name = d.product_string.as_deref().unwrap_or("X56 Device");
    println!("• {} [{} - {}]", name, d.vendor, d.kind);
    println!("  Ruta hidraw : {}", d.path.to_string_lossy());
    println!("  USB ID      : {:04x}:{:04x}", d.vendor_id, d.product_id);
    if let Some(ref s) = d.serial_number {
        println!("  Nº Serie    : {}", s);
    }
    println!();
}

fn handle_rgb(args: RgbArgs) -> ExitCode {
    let color = match parse_color(&args.color) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Error: {}", err);
            return ExitCode::FAILURE;
        }
    };

    let target: TargetDevice = args.device.into();
    match X56Manager::set_rgb(target, color) {
        Ok(count) => {
            let hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
            match target {
                TargetDevice::All => {
                    println!(
                        "Color RGB {} aplicado exitosamente en {} dispositivo(s).",
                        hex, count
                    );
                }
                TargetDevice::Stick => {
                    println!("Color RGB {} aplicado en el Stick.", hex);
                }
                TargetDevice::Throttle => {
                    println!("Color RGB {} aplicado en el Throttle.", hex);
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error aplicando color RGB: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_axis_get(device: DeviceKind, axis_str: &str, json: bool) -> ExitCode {
    match X56Manager::get_axis_config(device, axis_str) {
        Ok(cfg) => {
            let name = axis_name(cfg.axis_id, device);

            if json {
                let json_obj = serde_json::json!({
                    "device": device.to_string(),
                    "axis_id": cfg.axis_id,
                    "axis_name": name,
                    "xsat": cfg.xsat,
                    "ysat": cfg.ysat,
                    "deadband": cfg.deadband,
                    "curvature": cfg.curvature,
                    "profile": match cfg.profile {
                        CurveProfile::S => "s",
                        CurveProfile::J => "j",
                    },
                    "hall_zero": format!("{:#06x}", cfg.hall_zero),
                });
                println!("{}", serde_json::to_string_pretty(&json_obj).unwrap());
            } else {
                println!("Configuración de eje: {} ({})", name, device);
                println!("--------------------------------------------------");
                println!("  ID del eje           : {}", cfg.axis_id);
                println!("  Saturación X (xsat)  : {} / 1000", cfg.xsat);
                println!("  Saturación Y (ysat)  : {} / 1000", cfg.ysat);
                println!("  Zona muerta (deadband: {} / 1000", cfg.deadband);
                println!("  Curvatura (curvature): {} / 1000", cfg.curvature);
                println!(
                    "  Perfil de curva      : {}",
                    match cfg.profile {
                        CurveProfile::S => "S (Curva simétrica estándar)",
                        CurveProfile::J => "J (Curva en J / Progresiva)",
                    }
                );
                println!(
                    "  Calibración Hall     : {:#06x} ({})",
                    cfg.hall_zero, cfg.hall_zero
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error consultando eje: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_axis_set(args: AxisSetArgs) -> ExitCode {
    let device: DeviceKind = args.device.into();
    let profile = args.profile.map(Into::into);
    match X56Manager::set_axis_config(
        device,
        &args.axis,
        args.xsat,
        args.ysat,
        args.deadband,
        args.curvature,
        profile,
    ) {
        Ok(cfg) => {
            let name = axis_name(cfg.axis_id, device);
            if args.json {
                let json_obj = serde_json::json!({
                    "status": "updated",
                    "device": device.to_string(),
                    "axis_id": cfg.axis_id,
                    "axis_name": name,
                    "xsat": cfg.xsat,
                    "ysat": cfg.ysat,
                    "deadband": cfg.deadband,
                    "curvature": cfg.curvature,
                    "profile": match cfg.profile {
                        CurveProfile::S => "s",
                        CurveProfile::J => "j",
                    },
                    "hall_zero": format!("{:#06x}", cfg.hall_zero),
                });
                println!("{}", serde_json::to_string_pretty(&json_obj).unwrap());
            } else {
                println!("Ajustes aplicados con éxito para: {} ({})", name, device);
                println!("--------------------------------------------------");
                println!("  Saturación X : {} / 1000", cfg.xsat);
                println!("  Saturación Y : {} / 1000", cfg.ysat);
                println!("  Zona muerta  : {} / 1000", cfg.deadband);
                println!("  Curvatura    : {} / 1000", cfg.curvature);
                println!(
                    "  Perfil       : {}",
                    match cfg.profile {
                        CurveProfile::S => "S (Simétrica)",
                        CurveProfile::J => "J (Asimétrica)",
                    }
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error configurando eje: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_doctor(json: bool) -> ExitCode {
    let report = Doctor::run();

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return if report.is_healthy() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    println!("Diagnóstico del Sistema Saitek / Logitech X56 (x56 doctor)");
    println!("==========================================================");
    for check in &report.checks {
        let tag = match check.status {
            CheckStatus::Ok => "✓ [OK]  ",
            CheckStatus::Warn => "! [WARN]",
            CheckStatus::Fail => "✗ [FAIL]",
        };
        println!("{} {}: {}", tag, check.title, check.message);
        if let Some(ref remedy) = check.remedy {
            println!("         ↳ Solución sugerida: {}", remedy);
        }
    }
    println!("----------------------------------------------------------");
    println!(
        "Resumen: {} OK, {} advertencias, {} fallos.",
        report.ok_count, report.warn_count, report.fail_count
    );

    if report.is_healthy() {
        println!("Estado del sistema: 100% OPERATIVO.");
        ExitCode::SUCCESS
    } else {
        println!("Se detectaron problemas que impiden el funcionamiento óptimo sin privilegios.");
        ExitCode::FAILURE
    }
}

fn handle_profile_save(
    path: PathBuf,
    name: Option<String>,
    stick_rgb: Option<String>,
    throttle_rgb: Option<String>,
) -> ExitCode {
    let profile_name = name.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "X56 Profile".into())
    });

    match ProfileManager::dump_current(&profile_name, stick_rgb.as_deref(), throttle_rgb.as_deref())
    {
        Ok(profile) => match profile.save_to_file(&path) {
            Ok(_) => {
                println!(
                    "Perfil '{}' guardado exitosamente en: {}",
                    profile.name,
                    path.display()
                );
                if let Some(ref s) = profile.stick {
                    println!("  • Palanca (Stick) : {} ejes exportados", s.axes.len());
                }
                if let Some(ref t) = profile.throttle {
                    println!(
                        "  • Acelerador (Throttle): {} ejes exportados",
                        t.axes.len()
                    );
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("Error escribiendo archivo de perfil: {}", err);
                ExitCode::FAILURE
            }
        },
        Err(err) => {
            eprintln!("Error volcando configuración de hardware: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_profile_load(path: PathBuf, json: bool) -> ExitCode {
    let profile = match FullProfile::load_from_file(&path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("Error leyendo archivo de perfil {:?}: {}", path, err);
            return ExitCode::FAILURE;
        }
    };

    match ProfileManager::apply(&profile) {
        Ok(summary) => {
            if json {
                let json_obj = serde_json::json!({
                    "status": "applied",
                    "profile_name": profile.name,
                    "file": path.display().to_string(),
                    "stick_axes_updated": summary.stick_axes_updated,
                    "throttle_axes_updated": summary.throttle_axes_updated,
                    "rgb_updated": summary.rgb_updated,
                });
                println!("{}", serde_json::to_string_pretty(&json_obj).unwrap());
            } else {
                println!("Perfil '{}' aplicado exitosamente:", profile.name);
                println!(
                    "  • Ejes actualizados en Stick    : {}",
                    summary.stick_axes_updated
                );
                println!(
                    "  • Ejes actualizados en Throttle : {}",
                    summary.throttle_axes_updated
                );
                if summary.rgb_updated {
                    println!("  • Iluminación RGB               : Actualizada según perfil");
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error aplicando perfil al hardware: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_reset_defaults(target: TargetDevice) -> ExitCode {
    match ProfileManager::reset_defaults(target) {
        Ok(_) => {
            match target {
                TargetDevice::All => {
                    println!("Valores predeterminados de fábrica restaurados en Stick y Throttle.")
                }
                TargetDevice::Stick => {
                    println!("Valores predeterminados de fábrica restaurados en el Stick.")
                }
                TargetDevice::Throttle => {
                    println!("Valores predeterminados de fábrica restaurados en el Throttle.")
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error restaurando valores de fábrica: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_calibrate(args: CalibrateArgs) -> ExitCode {
    if args.device != CliDeviceKind::Stick {
        eprintln!("Error: La calibración de sensores de efecto Hall está disponible únicamente para la palanca (Stick).");
        return ExitCode::FAILURE;
    }

    match Calibrator::calibrate_stick(args.samples, args.dry_run) {
        Ok(results) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&results).unwrap());
                return ExitCode::SUCCESS;
            }

            println!("Calibración de Sensores de Efecto Hall (Stick X e Y)");
            println!("====================================================");
            if args.dry_run {
                println!("MODO SIMULACIÓN (DRY-RUN): Ningún dato fue escrito en la EEPROM.");
            } else {
                println!("MODO ACTIVO: Puntos cero aplicados y guardados en el hardware.");
            }
            println!(
                "Muestras analizadas: {} paquetes desde la Interfaz 0 (solo lectura)",
                args.samples
            );
            println!();

            for r in &results {
                println!("• {}", r.axis_name);
                println!(
                    "  Punto cero anterior  : {:#06x} ({})",
                    r.old_hall_zero, r.old_hall_zero
                );
                println!(
                    "  Punto cero muestreado: {:#06x} ({})",
                    r.sampled_hall_zero, r.sampled_hall_zero
                );
                println!("  Ajuste (Delta)       : {:+}", r.delta);
                println!(
                    "  Estado               : {}",
                    if r.applied {
                        "APLICADO"
                    } else {
                        "PENDIENTE (dry-run)"
                    }
                );
                println!();
            }

            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error ejecutando calibración: {}", err);
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// Subcomando `buttons`
// ---------------------------------------------------------------------------

fn device_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Stick => "stick",
        DeviceKind::Throttle => "throttle",
    }
}

/// Código evdev en hexadecimal; el POV (código 0) se lee de `ABS_HAT0X/Y`.
fn evdev_display(d: &ButtonDescriptor) -> String {
    match abs_axis(d) {
        Some(axis) => axis.to_string(),
        None => format!("{:#05x}", d.evdev_code),
    }
}

fn abs_axis(d: &ButtonDescriptor) -> Option<&'static str> {
    if d.evdev_code != 0 {
        return None;
    }
    match d.index {
        100 | 102 => Some("ABS_HAT0Y"),
        _ => Some("ABS_HAT0X"),
    }
}

fn handle_buttons_list(device: CliTargetDevice, json: bool) -> ExitCode {
    let descriptors = match device {
        CliTargetDevice::All => all_button_descriptors(),
        CliTargetDevice::Stick => catalog_for_device(DeviceKind::Stick),
        CliTargetDevice::Throttle => catalog_for_device(DeviceKind::Throttle),
    };

    if json {
        let list: Vec<serde_json::Value> = descriptors
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.id,
                    "device": device_name(d.device),
                    "index": d.index,
                    "code_label": d.code_label,
                    "evdev_code": evdev_display(d),
                    "group": d.group,
                    "label": d.label,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&list).unwrap());
        return ExitCode::SUCCESS;
    }

    let id_w = descriptors
        .iter()
        .map(|d| d.id.len())
        .max()
        .unwrap_or(2)
        .max(2);
    let ev_w = descriptors
        .iter()
        .map(|d| evdev_display(d).len())
        .max()
        .unwrap_or(5)
        .max(5);
    let grp_w = descriptors
        .iter()
        .map(|d| d.group.chars().count())
        .max()
        .unwrap_or(5)
        .max(5);

    println!(
        "{:<id_w$}  {:<3}  {:<ev_w$}  {:<grp_w$}  NOMBRE",
        "ID", "SEC", "EVDEV", "GRUPO"
    );
    let mut current: Option<DeviceKind> = None;
    for d in &descriptors {
        if current != Some(d.device) {
            current = Some(d.device);
            println!("-- {} --", device_name(d.device).to_uppercase());
        }
        let pad = grp_w.saturating_sub(d.group.chars().count());
        println!(
            "{:<id_w$}  {:<3}  {:<ev_w$}  {}{}  {}",
            d.id,
            d.code_label,
            evdev_display(d),
            d.group,
            " ".repeat(pad),
            d.label
        );
    }
    println!();
    println!(
        "Total: {} botones. Selector de modo F (M1/M2/S1) = 0x2d1/0x2d2/0x2d3.",
        descriptors.len()
    );
    ExitCode::SUCCESS
}

fn handle_buttons_status(json: bool) -> ExitCode {
    let monitor = ButtonMonitor::new();
    // Deja que el hilo poller lea el estado real de evdev.
    thread::sleep(Duration::from_millis(60));
    let snap = monitor.snapshot();
    drop(monitor);

    let all = all_button_descriptors();
    let mut pressed: Vec<&String> = snap.pressed_buttons.iter().collect();
    pressed.sort();

    if json {
        let obj = serde_json::json!({
            "active_mode": snap.active_mode.as_str(),
            "active_mode_description": snap.active_mode.description(),
            "slider_sld": if snap.slider_sld_on { "ON" } else { "OFF" },
            "stick_connected": snap.stick_connected,
            "throttle_connected": snap.throttle_connected,
            "pressed_buttons": pressed,
        });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
        return ExitCode::SUCCESS;
    }

    let conn = |c: bool| if c { "CONECTADO" } else { "NO DETECTADO" };
    println!("Estado en vivo del X56 (evdev)");
    println!("------------------------------");
    println!("Stick    : {}", conn(snap.stick_connected));
    println!("Throttle : {}", conn(snap.throttle_connected));
    println!(
        "Modo     : {} - {}",
        snap.active_mode.as_str(),
        snap.active_mode.description()
    );
    println!(
        "SLD      : {}",
        if snap.slider_sld_on { "ON" } else { "OFF" }
    );
    if pressed.is_empty() {
        println!("Botones pulsados: ninguno");
    } else {
        println!("Botones pulsados ({}):", pressed.len());
        for id in pressed {
            let label = all
                .iter()
                .find(|d| &d.id == id)
                .map(|d| d.label)
                .unwrap_or("?");
            println!("  • {id} ({label})");
        }
    }
    if !snap.stick_connected && !snap.throttle_connected {
        eprintln!(
            "Aviso: no se pudo leer ningún nodo /dev/input/event* del X56 (¿desconectado o sin permisos?)."
        );
    }
    ExitCode::SUCCESS
}

/// Divide una lista de teclas separadas por `+` o `,`.
fn parse_keys(input: &str) -> Vec<String> {
    input
        .split(['+', ','])
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .collect()
}

/// Valida el botón y asigna la celda en el modo indicado; devuelve el descriptor del botón.
fn apply_bind(
    profile: &mut FullProfile,
    mode: ModeSlot,
    button_id: &str,
    keys: Vec<String>,
    name: Option<String>,
) -> Result<ButtonDescriptor, String> {
    let descriptor = all_button_descriptors()
        .into_iter()
        .find(|d| d.id == button_id)
        .ok_or_else(|| {
            format!("botón desconocido '{button_id}' (usa `x56 buttons list` para ver los IDs)")
        })?;
    if keys.is_empty() {
        return Err("--keys no contiene ninguna tecla".to_string());
    }
    let cell = CommandCell {
        button_id: descriptor.id.clone(),
        name: name.unwrap_or_else(|| descriptor.label.to_string()),
        keys,
        steps: Vec::new(),
    };
    profile
        .programming
        .get_or_insert_with(ProgrammingProfile::default)
        .set_cell(mode, cell);
    Ok(descriptor)
}

fn handle_buttons_bind(args: ButtonsBindArgs) -> ExitCode {
    let mode: ModeSlot = args.mode.into();

    let mut profile = if args.file.exists() {
        match FullProfile::load_from_file(&args.file) {
            Ok(p) => p,
            Err(err) => {
                eprintln!("Error cargando perfil {}: {}", args.file.display(), err);
                return ExitCode::FAILURE;
            }
        }
    } else {
        let name = args
            .file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "x56-profile".to_string());
        FullProfile::new(name)
    };

    let keys = parse_keys(&args.keys);
    let descriptor = match apply_bind(&mut profile, mode, &args.button, keys.clone(), args.name) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("Error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = profile.save_to_file(&args.file) {
        eprintln!("Error guardando perfil {}: {}", args.file.display(), err);
        return ExitCode::FAILURE;
    }

    println!(
        "[{}] {} ({}) -> {}",
        mode.as_str(),
        descriptor.id,
        descriptor.label,
        keys.join(" + ")
    );
    println!("Perfil guardado en {}", args.file.display());
    ExitCode::SUCCESS
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_signum: i32) {
    STOP.store(true, Ordering::SeqCst);
}

extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
}

fn install_sigint_handler() {
    const SIGINT: i32 = 2;
    // SAFETY: el manejador solo escribe un `AtomicBool` (async-signal-safe).
    unsafe {
        signal(SIGINT, on_sigint);
    }
}

fn handle_buttons_run(path: &Path) -> ExitCode {
    let profile = match FullProfile::load_from_file(path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("Error cargando perfil {}: {}", path.display(), err);
            return ExitCode::FAILURE;
        }
    };
    let programming = profile.programming.unwrap_or_else(|| {
        eprintln!("Aviso: el perfil no contiene asignaciones de botones ([programming]).");
        ProgrammingProfile::default()
    });
    let assigned = programming.total_assigned_count();

    install_sigint_handler();
    let monitor = ButtonMonitor::new();
    let fallback_stick = profile.stick.as_ref().and_then(|s| s.rgb.clone());
    let fallback_throttle = profile.throttle.as_ref().and_then(|t| t.rgb.clone());
    monitor.set_fallback_rgb(fallback_stick, fallback_throttle);
    monitor.set_profile(programming.clone());
    monitor.set_active(true);

    println!(
        "Perfil '{}' activo: {} asignación(es). Ctrl+C para salir.",
        profile.name, assigned
    );
    if !monitor.uinput_available() {
        eprintln!(
            "Aviso: /dev/uinput no disponible; los eventos se registrarán pero no se emitirán teclas."
        );
    }

    let mode_desc = |m: ModeSlot| -> String {
        let name_str = programming
            .bindings_for(m)
            .name
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        let rgb_str = programming
            .rgb_for_mode(m, DeviceKind::Stick)
            .map(|c| format!(" [RGB: {c}]"))
            .unwrap_or_default();
        format!("{}: {}{}{}", m.as_str(), m.description(), name_str, rgb_str)
    };

    thread::sleep(Duration::from_millis(60));
    let mut last_mode = monitor.snapshot().active_mode;
    println!("Modo actual: {}", mode_desc(last_mode));

    while !STOP.load(Ordering::SeqCst) {
        let mode = monitor.snapshot().active_mode;
        if mode != last_mode {
            println!("Modo -> {}", mode_desc(mode));
            last_mode = mode;
        }
        for ev in monitor.drain_events() {
            println!(
                "[{}] [{}] {} ({}) -> {} [{}] {}",
                ev.timestamp_hms,
                ev.mode.as_str(),
                ev.button_id,
                ev.button_label,
                ev.command_name,
                ev.keys.join(" + "),
                if ev.uinput_emitted {
                    "uinput: emitido"
                } else {
                    "uinput: NO emitido"
                }
            );
        }
        thread::sleep(Duration::from_millis(20));
    }

    monitor.set_active(false);
    println!("\nDetenido.");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_buttons_subcommands() {
        assert!(
            Cli::try_parse_from(["x56", "buttons", "list", "--device", "stick", "--json"]).is_ok()
        );
        assert!(Cli::try_parse_from(["x56", "buttons", "status", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["x56", "buttons", "run", "p.toml"]).is_ok());
        assert!(Cli::try_parse_from([
            "x56",
            "buttons",
            "bind",
            "--file",
            "p.toml",
            "--mode",
            "m2",
            "--button",
            "stick_trigger",
            "--keys",
            "Ctrl+F",
            "--name",
            "Fuego",
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "x56", "buttons", "bind", "--file", "p.toml", "--mode", "m9", "--button", "x",
            "--keys", "A",
        ])
        .is_err());
    }

    #[test]
    fn keys_split_on_plus_and_comma() {
        assert_eq!(parse_keys("Ctrl+Shift, F5"), ["Ctrl", "Shift", "F5"]);
        assert_eq!(parse_keys(" A "), ["A"]);
        assert!(parse_keys("+ , ").is_empty());
    }

    #[test]
    fn bind_validates_and_assigns() {
        let mut p = FullProfile::new("t");
        let d = apply_bind(&mut p, ModeSlot::S1, "thr_sld", vec!["G".into()], None).unwrap();
        assert_eq!(d.id, "thr_sld");
        let cell = p
            .programming
            .as_ref()
            .unwrap()
            .get_cell(ModeSlot::S1, "thr_sld")
            .unwrap();
        assert_eq!(cell.name, d.label);
        assert_eq!(cell.keys, ["G"]);

        assert!(apply_bind(&mut p, ModeSlot::M1, "nope", vec!["A".into()], None).is_err());
        assert!(apply_bind(&mut p, ModeSlot::M1, "thr_sld", vec![], None).is_err());
        assert_eq!(p.programming.unwrap().total_assigned_count(), 1);
    }

    #[test]
    fn evdev_display_formats() {
        let all = all_button_descriptors();
        let get = |id: &str| all.iter().find(|d| d.id == id).unwrap();
        assert_eq!(evdev_display(get("stick_trigger")), "0x120");
        assert_eq!(evdev_display(get("thr_sld")), "0x2d0");
        assert_eq!(evdev_display(get("stick_pov_up")), "ABS_HAT0Y");
        assert_eq!(evdev_display(get("stick_pov_left")), "ABS_HAT0X");
    }
}
