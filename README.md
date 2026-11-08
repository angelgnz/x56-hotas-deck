# x56linux (v0.1.0)

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-green.svg)]()

Herramienta moderna, segura y de alto rendimiento en Rust para la configuración de iluminación LED RGB, curvas de respuesta de ejes, zonas muertas y calibración de hardware en el sistema HOTAS **Saitek Pro Flight X-56 Rhino** y **Logitech G X56**.

Diseñada específicamente para operar en Linux **sin privilegios de root (`sudo`)** y **sin desconectar el driver del joystick del kernel (`usbhid`/`joydev`/`evdev`)**, garantizando que simuladores de vuelo (DCS World, X-Plane, Microsoft Flight Simulator, Star Citizen, Elite Dangerous) sigan recibiendo entradas de ejes y botones de forma continua mientras se ajusta la configuración.

---

## Características

- **Acceso sin root**: Utiliza `hidraw` con la regla udev moderna `uaccess` sobre la interfaz USB 2 de configuración (`0xFF00`), dejando la interfaz 0 intacta para el juego.
- **Control de Iluminación RGB**: Soporte para colores hexadecimales (`#ff8800`) o por nombre común (`cyan`, `orange`, `red`, etc.) para Stick, Throttle o ambos simultáneamente.
- **Gestión Completa de Ejes**: Consulta y modificación de saturación X/Y, zonas muertas (deadband), curvatura (lineal o personalizada) y perfiles de curva (S simétrica o J progresiva).
- **Perfiles en TOML**: Exporta y aplica perfiles completos de configuración en formato TOML legible y versionado.
- **Calibración de Sensores Hall**: Asistente de muestreo y calibración del punto cero físico de los sensores magnéticos del Stick mediante lectura no intrusiva de la interfaz 0 con opción `--dry-run`.
- **Diagnóstico Integral (`x56 doctor`)**: Verifica en segundos reglas udev, permisos de usuario, conectividad USB, soporte del kernel y handshake con el firmware.
- **Salida Dual**: Tablas legibles en consola o salida estructurada `--json` para scripts, integración y futuras interfaces gráficas.

---

## Dispositivos Soportados

| Fabricante | Periférico | USB VID:PID | Rol |
|---|---|---|---|
| **Saitek / Mad Catz** | Pro Flight X-56 Rhino Stick | `0738:2221` | Palanca principal (Roll, Pitch, Twist/Yaw) |
| **Saitek / Mad Catz** | Pro Flight X-56 Rhino Throttle | `0738:a221` | Gases duales y 4 ruedas rotatorias |
| **Logitech G** | X56 HOTAS Stick | `046d:c22b` | Palanca principal |
| **Logitech G** | X56 HOTAS Throttle | `046d:c22a` | Gases duales y rotatorios |

---

## Instalación y Configuración de Permisos

### 1. Regla udev (Acceso sin root)

Instala la regla udev provista para conceder permisos automáticos al usuario activo en el sistema:

```bash
sudo cp packaging/70-x56.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

### 2. Compilación del CLI

```bash
git clone https://github.com/angelgnz/x56linux.git
cd x56linux
cargo build --release
```

El binario resultante se encontrará en `target/release/x56`. Puedes copiarlo a `~/.local/bin/` o `/usr/local/bin/`.

---

## Uso de la CLI

### Diagnóstico del sistema
Comprueba que el entorno, las reglas udev y los periféricos estén listos:
```bash
x56 doctor
x56 doctor --json
```

### Detección y lista de dispositivos
```bash
x56 list
x56 list --json
```

### Iluminación LED RGB
```bash
# Cambiar iluminación a color cian en ambos periféricos
x56 rgb cyan

# Cambiar color en un dispositivo específico mediante código hex
x56 rgb --device stick '#00ffff'
x56 rgb --device throttle '#ff5500'

# Apagar LEDs
x56 rgb off
```

### Consulta y modificación de curvas de ejes
```bash
# Consultar configuración de los ejes principales del Stick
x56 axis get --device stick --axis x
x56 axis get --device stick --axis y
x56 axis get --device stick --axis z --json

# Consultar ejes del Throttle
x56 axis get --device throttle --axis left
x56 axis get --device throttle --axis r1

# Modificar zona muerta y curvatura del eje X del Stick
x56 axis set --device stick --axis x --deadband 60 --curvature 520

# Modificar perfil de curva del acelerador izquierdo
x56 axis set --device throttle --axis left --curvature 480 --profile j
```

### Gestión de perfiles (TOML)
```bash
# Exportar la configuración actual de ambos dispositivos a un archivo TOML
x56 profile save mi_perfil.toml --name "DCS Hornet"

# Cargar y aplicar un perfil guardado
x56 profile load mi_perfil.toml

# Restaurar valores de fábrica originales
x56 reset-defaults --device all
```

En el directorio [`examples/`](examples/) se incluyen perfiles de plantilla listos para usar (`default_profile.toml`, `combat_profile.toml`).

### Calibración de Sensores Hall (Stick)
Muestrea y recalibra el centro magnético del joystick sin tocar la palanca:
```bash
# Simular muestreo (dry-run) sin escribir en la EEPROM
x56 calibrate stick --dry-run

# Aplicar calibración física con 100 muestras
x56 calibrate stick --samples 100
```

---

## Arquitectura del Workspace

El proyecto está estructurado como un workspace modular en Rust:

- **[`crates/x56-proto`](crates/x56-proto/)**: Codificación y decodificación binaria estricta de todos los Feature Reports (0x01, 0x08, 0x09, 0x0b) e Interrupt Reports (0xFF). Sin dependencias externas, `no_std`-friendly.
- **[`crates/x56-hid`](crates/x56-hid/)**: Capa de transporte HID (`hidraw`) basada en `hidapi`. Prohíbe tajantemente la apertura de la Interfaz 0 y gestiona la cadencia y acknowledge de transferencias en la Interfaz 2.
- **[`crates/x56-core`](crates/x56-core/)**: Lógica de dominio, perfiles TOML, asistente de calibración y motor de diagnóstico.
- **[`crates/x56-cli`](crates/x56-cli/)**: Interfaz de línea de comandos basada en `clap` con salidas formateadas y soporte `--json`.

---

## Licencia

Este software se distribuye bajo los términos de la Licencia Pública General GNU versión 3 (GPLv3). Consulta el archivo [`LICENSE`](LICENSE) para más detalles.
