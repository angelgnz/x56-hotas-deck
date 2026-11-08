# x56linux

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux-green.svg)]()
[![Tests: 25 passing](https://img.shields.io/badge/tests-25%20passed-brightgreen.svg)]()

Herramienta moderna, segura y de alto rendimiento en **Rust** para la configuración de iluminación LED RGB, curvas de respuesta de ejes, zonas muertas y calibración de hardware en el sistema HOTAS **Saitek / Mad Catz Pro Flight X-56 Rhino** y **Logitech G X56**.

Diseñada específicamente para operar en Linux **sin privilegios de root (`sudo`)** y **sin desconectar el driver del joystick del kernel (`usbhid`/`joydev`/`evdev`)**, garantizando que simuladores de vuelo y espaciales (*DCS World, X-Plane, Microsoft Flight Simulator, Star Citizen, Elite Dangerous, FlightGear*) sigan recibiendo entradas de ejes y botones ininterrumpidamente mientras se ajusta la configuración en vivo.

---

## Índice

- [Motivación y Mejoras sobre la Base C++ Original](#motivación-y-mejoras-sobre-la-base-c-original)
- [Características Principales](#características-principales)
- [Dispositivos Soportados](#dispositivos-soportados)
- [Mapeo Canónico de Ejes y Rangos](#mapeo-canónico-de-ejes-y-rangos)
- [Instalación y Configuración](#instalación-y-configuración)
- [Guía de Uso del CLI](#guía-de-uso-del-cli)
  - [Diagnóstico del Sistema (`x56 doctor`)](#1-diagnóstico-del-sistema-x56-doctor)
  - [Detección de Dispositivos (`x56 list`)](#2-detección-de-dispositivos-x56-list)
  - [Iluminación LED RGB (`x56 rgb`)](#3-iluminación-led-rgb-x56-rgb)
  - [Curvas y Zonas Muertas (`x56 axis`)](#4-curvas-y-zonas-muertas-x56-axis)
  - [Perfiles TOML (`x56 profile`)](#5-perfiles-toml-x56-profile)
  - [Calibración de Sensores Hall (`x56 calibrate`)](#6-calibración-de-sensores-hall-x56-calibrate)
  - [Restauración de Fábrica (`x56 reset-defaults`)](#7-restauración-de-fábrica-x56-reset-defaults)
- [Centro de Control Gráfico (`x56-gui`)](#centro-de-control-gráfico-x56-gui)
  - [Editor de Curvas con Muestreo en Vivo](#1-editor-de-curvas-con-muestreo-en-vivo)
  - [Gestor de Iluminación RGB](#2-gestor-de-iluminación-rgb)
  - [Asistente de Calibración Hall](#3-asistente-de-calibración-hall)
  - [Diagnóstico del Sistema Integrado](#4-diagnóstico-del-sistema-integrado)
- [Especificación del Protocolo Binario HID](#especificación-del-protocolo-binario-hid)
- [Arquitectura del Workspace](#arquitectura-del-workspace)
- [Estado del Roadmap](#estado-del-roadmap)
- [Licencia](#licencia)

---

## Motivación y Mejoras sobre la Base C++ Original

El proyecto original (`Chryseus/x56linux`), escrito en C++ en 2016, quedó abandonado con importantes problemas de arquitectura y limitaciones críticas de funcionamiento:

| Problema en código C++ original | Solución en `x56linux` (Rust) |
|---|---|
| **Desconexión forzada del kernel (`libusb_detach_kernel_driver`)**: Mataba la Interfaz 0, congelando el joystick en simuladores durante el vuelo. | **Cero detach**: La Interfaz 0 se mantiene en el kernel (`joydev`/`evdev`), mientras toda configuración se canaliza exclusivamente por la Interfaz 2 (`0xFF00`) mediante `hidraw`. |
| **Requiere `sudo` / `root`**: No ofrecía integración udev moderna, forzando la ejecución con privilegios elevados. | **Acceso sin root**: Regla udev moderna con `TAG+="uaccess"` y grupo de respaldo `input` (`0660`). |
| **Buffer Overflow en USB**: Reservaba buffers de 63 bytes pero enviaba 64 bytes (`wLength = 64`), provocando desbordamientos de memoria. | **Tipado fuerte y buffers de longitud exacta**: `[u8; 64]` constante verificado en tiempo de compilación. |
| **`wIndex` incorrecto**: Desplazaba el índice de interfaz erróneamente (`(2 << 8)` en vez de `0x0002`). | **Offsets exactos validados contra hardware real** y report descriptors HID del dispositivo. |
| **Escritura de ejes no implementada**: Solo implementaba RGB; la escritura de curvas estaba ausente y fallaba. | **Protocolo completo de 4 fases con cadencia (pacing)** verificado en hardware físico con round-trip de lectura. |
| **IDs de ejes ambiguos**: Confusión entre numeración decimal y códigos de uso estándar USB HID. | **Resolución canónica dual**: Soporta nombres textuales (`x`, `pitch`, `left`), hexadecimal (`0x30`) y decimales. |

---

## Características Principales

- **Acceso seguro sin root**: Gestionado automáticamente por `logind` y `uaccess` al iniciar sesión.
- **Control de Iluminación RGB**: Soporte para colores hexadecimales (`#00ffff`), shorthand (`#0ff`) o nombres comunes (`cyan`, `orange`, `red`, `off`, etc.) para Stick, Throttle o ambos periféricos simultáneamente.
- **Curvas de respuesta de alta resolución**: Rango matemático `0..=1000` (500 = lineal) para saturación X/Y, curvatura y deadband con curvas simétricas (tipo **S**) o progresivas (tipo **J**).
- **Gestión de Perfiles en TOML**: Exportación e importación de perfiles completos de configuración en formato legible y versionado.
- **Calibración Hall no invasiva**: Muestreo pasivo de la posición física central en la Interfaz 0 (`/dev/hidraw*`), cálculo de desviaciones (delta) y modo `--dry-run` seguro sin riesgo de descalibración accidental.
- **Motor de Diagnóstico (`x56 doctor`)**: Comprobación integral de 6 capas (udev, permisos, hardware USB, hidraw, kernel joydev y handshake de firmware).
- **Salida dual humana / JSON**: Todas las operaciones admiten `--json` para scripts de automatización, integración continua o interfaces gráficas.

---

## Dispositivos Soportados

| Fabricante | Periférico | USB VID:PID | Rol / Ejes Principales |
|---|---|---|---|
| **Saitek / Mad Catz** | Pro Flight X-56 Rhino Stick | `0738:2221` | Palanca principal (Roll, Pitch, Twist/Yaw) |
| **Saitek / Mad Catz** | Pro Flight X-56 Rhino Throttle | `0738:a221` | Gases duales independientes y 4 rotatorios |
| **Logitech G** | X56 HOTAS Stick | `046d:c22b` | Palanca principal (versión negra/gris) |
| **Logitech G** | X56 HOTAS Throttle | `046d:c22a` | Gases duales y rotatorios |

---

## Mapeo Canónico de Ejes y Rangos

El protocolo utiliza los códigos de uso USB HID Generic Desktop (en formato hexadecimal):

### Palanca (Stick)
| Eje | Nombres aceptados en CLI | ID Hex | ID Dec | Perfil Predeterminado | Deadband de Fábrica |
|---|---|---|---|---|---|
| **X (Roll / Alabeo)** | `x`, `roll` | `0x30` | `48` | `S` (Simétrico) | 60 / 1000 |
| **Y (Pitch / Cabeceo)** | `y`, `pitch` | `0x31` | `49` | `S` (Simétrico) | 50 / 1000 |
| **Z (Twist / Guiñada)** | `z`, `yaw`, `twist`, `rudder` | `0x35` | `53` | `S` (Simétrico) | 0 / 1000 |

### Cuadrante de Gases (Throttle)
| Eje | Nombres aceptados en CLI | ID Hex | ID Dec | Perfil Predeterminado | Deadband de Fábrica |
|---|---|---|---|---|---|
| **Throttle Left** | `left`, `throttle_left`, `tl`, `t1` | `0x30` | `48` | `J` (Asimétrico/Progresivo) | 0 / 1000 |
| **Throttle Right** | `right`, `throttle_right`, `tr`, `t2`| `0x31` | `49` | `J` (Asimétrico/Progresivo) | 0 / 1000 |
| **Rotary 1** | `rotary1`, `r1` | `0x32` | `50` | `S` (Simétrico) | 0 / 1000 |
| **Rotary 2** | `rotary2`, `r2` | `0x35` | `53` | `S` (Simétrico) | 0 / 1000 |
| **Rotary 4** | `rotary4`, `r4` | `0x36` | `54` | `S` (Simétrico) | 0 / 1000 |
| **Rotary 3** | `rotary3`, `r3` | `0x37` | `55` | `S` (Simétrico) | 0 / 1000 |

> **Escala de parámetros**: Todos los parámetros de configuración (`xsat`, `ysat`, `deadband`, `curvature`) operan en el rango `0..=1000`. Un valor de `curvature = 500` representa una respuesta lineal 1:1.

---

## Instalación y Configuración

### 1. Regla udev (Acceso sin root)

Copia la regla de permisos provista y recarga udev:

```bash
sudo cp packaging/70-x56.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

### 2. Compilación e Instalación del Binario

Requiere toolchain de Rust estable (Rust 1.75+):

```bash
git clone https://github.com/angelgnz/x56linux.git
cd x56linux
cargo build --release
```

El ejecutable optimizado se genera en `target/release/x56`. Puedes enlazarlo en tu ruta de usuario:

```bash
cp target/release/x56 ~/.local/bin/
```

---

## Guía de Uso del CLI

### 1. Diagnóstico del Sistema (`x56 doctor`)

Verifica en un solo paso que tu sistema cumpla todos los requisitos:

```bash
x56 doctor
```

Salida típica:
```text
Diagnóstico del Sistema Saitek / Logitech X56 (x56 doctor)
==========================================================
✓ [OK]   Regla udev para acceso sin root: Regla activa y completa en /etc/udev/rules.d/70-x56.rules
✓ [OK]   Grupo 'input': El usuario actual pertenece al grupo 'input' (acceso headless garantizado)
✓ [OK]   Periféricos HOTAS detectados: Detectados: Stick: CONECTADO, Throttle: CONECTADO
✓ [OK]   Permisos de lectura y escritura: Acceso concedido sin root en todos los nodos hidraw (4 nodos verificados)
✓ [OK]   Driver de joystick del kernel (Interfaz 0): Nodos /dev/input/js* presentes; simuladores de vuelo recibirán ejes continuamente
✓ [OK]   Handshake con firmware X56 (Interfaz 2): Comunicación bidireccional confirmada en 2 dispositivo(s)
----------------------------------------------------------
Resumen: 6 OK, 0 advertencias, 0 fallos.
Estado del sistema: 100% OPERATIVO.
```

### 2. Detección de Dispositivos (`x56 list`)

Lista los dispositivos X56 detectados exclusivamente sobre la Interfaz 2:

```bash
x56 list
x56 list --json
```

### 3. Iluminación LED RGB (`x56 rgb`)

Control de iluminación LED en Stick, Throttle o ambos:

```bash
# Color cian de fábrica en ambos dispositivos
x56 rgb cyan

# Ajustar Stick en cian y Throttle en naranja mediante código hexadecimal
x56 rgb --device stick '#00ffff'
x56 rgb --device throttle '#ff5500'

# Apagar iluminación por completo
x56 rgb off
```

### 4. Curvas y Zonas Muertas (`x56 axis`)

#### Consultar la configuración actual de un eje:
```bash
# Ejes de la palanca
x56 axis get --device stick --axis x
x56 axis get --device stick --axis y
x56 axis get --device stick --axis z --json

# Ejes de los gases
x56 axis get --device throttle --axis left
x56 axis get --device throttle --axis r1
```

#### Modificar parámetros de un eje:
```bash
# Ajustar zona muerta a 65 y curvatura a 520 en el eje X del Stick
x56 axis set --device stick --axis x --deadband 65 --curvature 520

# Cambiar curva del acelerador izquierdo a progresiva (J)
x56 axis set --device throttle --axis left --curvature 480 --profile j
```

### 5. Perfiles TOML (`x56 profile`)

#### Exportar configuración actual del hardware:
```bash
x56 profile save mi_perfil.toml --name "DCS Hornet" --stick-rgb '#00ffff' --throttle-rgb '#ff5500'
```

#### Cargar y aplicar un perfil:
```bash
x56 profile load mi_perfil.toml
x56 profile load mi_perfil.toml --json
```

En la carpeta [`examples/`](examples/) se incluyen perfiles de muestra listos para usar:
- [`default_profile.toml`](examples/default_profile.toml): Curvas lineales y zonas muertas de fábrica.
- [`combat_profile.toml`](examples/combat_profile.toml): Zonas muertas reducidas, curvas agresivas para combate y LEDs naranja táctico.

### 6. Calibración de Sensores Hall (`x56 calibrate`)

Muestrea pasivamente la posición central del joystick a través de la Interfaz 0 (sin alterar el driver) y calcula el ajuste exacto del punto cero del sensor Hall:

```bash
# Modo simulación (Dry-Run): Muestra los valores calculados sin escribir en la EEPROM
x56 calibrate stick --dry-run

# Aplicar calibración física real promediando 100 muestras
x56 calibrate stick --samples 100
```

Salida de ejemplo:
```text
Calibración de Sensores de Efecto Hall (Stick X e Y)
====================================================
MODO ACTIVO: Puntos cero aplicados y guardados en el hardware.
Muestras analizadas: 100 paquetes desde la Interfaz 0 (solo lectura)

• X (Roll)
  Punto cero anterior  : 0x8859 (34905)
  Punto cero muestreado: 0x7f40 (32576)
  Ajuste (Delta)       : -2329
  Estado               : APLICADO

• Y (Pitch)
  Punto cero anterior  : 0x83f7 (33783)
  Punto cero muestreado: 0x8737 (34615)
  Ajuste (Delta)       : +832
  Estado               : APLICADO
```

### 7. Restauración de Fábrica (`x56 reset-defaults`)

Restaura los valores de curva y deadband predeterminados por el fabricante:

```bash
x56 reset-defaults --device all
```

---

## Centro de Control Gráfico (`x56-gui`)

`x56-gui` es una interfaz gráfica moderna, nativa y de modo inmediato construida con **egui / eframe** y **egui_plot**. Se ejecuta a 30+ FPS en X11 o Wayland con renderizado OpenGL acelerado.

```bash
# Lanzar la aplicación gráfica
x56-gui

# O compilar y ejecutar directamente con Cargo
cargo run -p x56-gui --release
```

### 1. Editor de Curvas con Muestreo en Vivo
- **Visualización 2D reactiva**: Dibuja la función de transferencia matemática real calculando en tiempo real las transformaciones de curvatura (S/J), saturación X/Y y zona muerta (deadband).
- **Entrada física en vivo**: Un hilo poller en segundo plano lee pasivamente los reportes HID de la **Interfaz 0** (`/dev/hidraw*`) sin detach del driver del kernel, graficando un punto rojo animado en la posición exacta del joystick.
- **Controles interactivos**: Sliders para curvatura (`0..1000`), deadband (`0..1000`), saturación X/Y (`0..1000`) y selector de perfil (Simétrico S vs Progresivo J).
- **Botones de acción inmediata**:
  - `Aplicar al Hardware`: Escribe y comete la configuración en la EEPROM del periférico.
  - `Leer del Hardware`: Refresca los controles leyendo la configuración actual del firmware.
  - `Valores Lineales (1:1)`: Restablece rápidamente la curva a respuesta pura sin zona muerta.

### 2. Gestor de Iluminación RGB
- Selector de color visual integrado (Color Picker RGB).
- Previsualización en tiempo real y aplicación instantánea en hardware.
- Sincronización automática opcional entre Stick y Throttle con un solo clic.
- Paleta de presets rápidos: *Rhino Cyan, Neon Green, Solar Orange, Deep Red, Pure White, Royal Purple, Stealth (Off)*.

### 3. Asistente de Calibración Hall
- Wizard guiado paso a paso para eliminar el centrado imperfecto y la deriva física en los sensores de Efecto Hall (ejes X e Y del Stick).
- Muestreo pasivo de 50 paquetes del centro físico sin interferir con simuladores activos.
- Cálculo de desviación (Delta) y evaluación de umbral de deriva (< 1000 = Excelente, 1000..3000 = Aceptable, > 3000 = Desviación Alta).
- Modo seguro `--dry-run` para previsualizar antes de cometer en la EEPROM.

### 4. Diagnóstico del Sistema Integrado
- Panel visual de comprobación del estado del sistema en tiempo real.
- Inspección de 6 capas: permisos udev, nodos `/dev/hidraw*`, detección de periféricos USB, soporte de simulador (`/dev/input/js*`) y handshake bidireccional con el firmware.

### 5. Gestión de Perfiles TOML
- Acceso directo en la barra superior para `Guardar Perfil TOML` o `Cargar Perfil TOML`.
- Permite respaldar perfiles completos y compartirlos entre diferentes simuladores de vuelo (*DCS*, *MSFS*, *X-Plane*, *Star Citizen*).

---

## Especificación del Protocolo Binario HID

El periférico Saitek/Logitech X56 expone dos interfaces USB principales:
- **Interfaz 0 (HID Class 3, SubClass 0)**: Canal de entrada de juego (`/dev/input/js*`). Endpoint 1 Interrupt IN (reportes de 11 bytes con la posición de los ejes y botones en Little-Endian).
- **Interfaz 2 (Vendor Specific 0xFF00)**: Canal de configuración bidireccional. Endpoint 0 (Control) y Endpoint 2 Interrupt IN.

### Report IDs en Interfaz 2
| Report ID | Tipo | Longitud | Propósito |
|---|---|---|---|
| `0x01` | Feature Set | 64 bytes | Packet 4: Commit / Fin de bloque de configuración |
| `0x08` | Feature Get | 64 bytes | Packet 6: Consulta de hardware (primer eje hardcodeado) |
| `0x09` | Feature Set | 64 bytes | Packet 5: Configuración de color RGB LED |
| `0x0B` | Feature Set | 64 bytes | Configuración y consulta de ejes (subcomandos 0x01, 0x02, 0x03, 0x04) |
| `0xFF` | Interrupt IN | 64 bytes | Packet 8: Respuesta asíncrona de consulta de eje |

### Secuencia de Escritura de Ejes (Reverse-Engineered)
Para aplicar de forma persistente la configuración de un eje:
1. **Packet 1 (Select Stage 1)**: Feature Report `0x0B`, subcomando `0x03` (`[0x0b, 0x03, 0x00, axis_id, ...]`).
2. **Pausa de cadencia**: `20 ms` de retardo.
3. **Packet 2 (Select Stage 2)**: Feature Report `0x0B`, subcomando `0x04` (`[0x0b, 0x04, 0x00, axis_id, ...]`).
4. **Pausa de cadencia**: `20 ms` de retardo. Drenaje no bloqueante del acknowledge en Interrupt IN.
5. **Packet 3 (Configuración)**: Feature Report `0x0B`, subcomando `0x01` con la carga útil completa en Big-Endian:
   - Bytes 8-9: `xsat`
   - Bytes 10-11: `ysat`
   - Bytes 12-13: `deadband`
   - Bytes 14-15: `curvature`
   - Byte 16: `profile` (`0` = S, `1` = J)
   - Bytes 17-18: `hall_zero`
6. **Packet 4 (Commit)**: Feature Report `0x01` (`[0x01, 0x01, ...]`).

---

## Arquitectura del Workspace

El proyecto está organizado en un workspace modular de Rust:

```text
x56linux/
├── Cargo.toml
├── packaging/
│   ├── 70-x56.rules             # Regla udev uaccess moderna
│   └── com.saitek.x56.desktop   # Lanzador de escritorio Linux
├── examples/
│   ├── default_profile.toml     # Perfil de fábrica
│   └── combat_profile.toml      # Perfil de combate para simuladores
└── crates/
    ├── x56-proto/               # Decodificador/codificador binario puro (no_std friendly)
    ├── x56-hid/                 # Transporte HID hidraw (aislamiento de interfaz 2)
    ├── x56-core/                # Dominio, perfiles TOML, calibrador Hall y doctor
    ├── x56-cli/                 # CLI basado en clap con salidas formateadas y --json
    └── x56-gui/                 # Centro de control gráfico nativo (egui / egui_plot)
```

---

## Estado del Roadmap

- [x] **Fase 0 — Base y Verificación**
  - [x] Workspace Cargo estructurado y rama `legacy/cpp` preservada.
  - [x] Laboratorio de captura e inspección hidraw en hardware real.
  - [x] Regla udev moderna `70-x56.rules` validada.
- [x] **Fase 1 — CLI Funcional y Protocolo Completo**
  - [x] `x56 list`: Detección en Interfaz 2 sin detach de joystick.
  - [x] `x56 rgb`: Control de luces LED por nombre y código hex.
  - [x] `x56 axis get`: Lectura de todos los ejes vía Interrupt Report 0xFF.
  - [x] `x56 axis set`: Modificación y round-trip persistente de curvas.
  - [x] `x56 profile save/load`: Perfiles TOML exportables y aplicables.
  - [x] `x56 calibrate`: Muestreo pasivo y calibración del sensor Hall.
  - [x] `x56 doctor`: Diagnóstico integral del sistema.
  - [x] `x56 reset-defaults`: Restauración de fábrica.
- [ ] **Fase 2 — Empaquetado y Servicio en Segundo Plano**
  - [ ] Generación de paquetes `.deb` y `.rpm`.
  - [ ] Servicio `systemd --user` para reaplicación automática de perfiles al reconectar.
- [x] **Fase 3 — Interfaz Gráfica (GUI)**
  - [x] Centro de control nativo en egui (`x56-gui`) a 30+ FPS.
  - [x] Editor visual interactivo de curvas 2D con `egui_plot`.
  - [x] Graficado en tiempo real del punto físico de entrada (muestreo pasivo en Interfaz 0).
  - [x] Gestor de color RGB con selector interactivo y sincronización Stick + Throttle.
  - [x] Asistente paso a paso de calibración de sensores Hall con umbrales de deriva.
  - [x] Pestaña de diagnóstico del sistema (`DoctorTab`) integrada.
  - [x] Modal de exportación / importación de perfiles TOML.

---

## Licencia

Este proyecto está bajo la Licencia Pública General de GNU v3.0 (GPLv3). Consulta el archivo [`LICENSE`](LICENSE) para los términos completos.
