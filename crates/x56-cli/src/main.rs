use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use x56_core::{
    axis_name, parse_color, CurveProfile, DeviceKind, TargetDevice, X56Descriptor, X56Manager,
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::List { json } => handle_list(json),
        Commands::Rgb(args) => handle_rgb(args),
        Commands::Axis { command } => match command {
            AxisCommands::Get { device, axis, json } => handle_axis_get(device.into(), &axis, json),
            AxisCommands::Set(args) => handle_axis_set(args),
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
