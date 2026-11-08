use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use x56_core::{
    axis_name, parse_color, Calibrator, CheckStatus, CurveProfile, DeviceKind, Doctor, FullProfile,
    ProfileManager, TargetDevice, X56Descriptor, X56Manager,
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
                    println!("  • Gases (Throttle): {} ejes exportados", t.axes.len());
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
