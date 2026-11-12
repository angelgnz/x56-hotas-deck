# Reglas del Proyecto x56linux (GEMINI.md)

Pautas, directrices y restricciones de arquitectura para el desarrollo y mantenimiento del proyecto `x56linux`.

---

## 1. Filosofía y Principios Fundamentales

- **Operación Rootless**: Todas las funcionalidades de `x56linux` (CLI, GUI, servicios) deben funcionar **sin privilegios de root (`sudo`)**, aprovechando las reglas udev con tag `uaccess` y el subsistema de permisos de Linux (`logind`).
- **Aislamiento Estricto de Interfaz (Invariante Crítica)**:
  - **Interfaz 0 (Juego / HID Class 3)**: Mapeada a `/dev/input/js*` y `/dev/input/event*`. **NUNCA** debe ser desconectada (`detach_kernel_driver`), desvinculada ni bloqueada. Los simuladores de vuelo deben continuar recibiendo telemetría física de ejes y botones sin interrupción.
  - **Interfaz 2 (Vendor Specific 0xFF00)**: Canal exclusivo para comandos de configuración bidireccional (iluminación RGB, curvas, zonas muertas, calibración Hall).
- **Cadencia de Comunicación Hardware**: Respetar siempre los tiempos de espera mínimos del protocolo binario (retardos de 20 ms entre etapas de paquetes de configuración) para evitar bloqueos del microcontrolador Saitek/Logitech.

---

## 2. Arquitectura del Workspace Cargo

El proyecto está organizado como un Cargo Workspace modular con responsabilidades bien delimitadas:

```text
crates/
├── x56-proto/   # Codificador/decodificador binario puro (no_std friendly, sin dependencias de SO)
├── x56-hid/     # Transporte y comunicación hidraw (aislamiento exclusivo a Interfaz 2)
├── x56-core/    # Lógica de dominio, perfiles TOML, calibración Hall, doctor y botones/modos
├── x56-cli/     # Interfaz de línea de comandos (clap, tablas formateadas, modo --json)
└── x56-gui/     # Centro de control gráfico nativo (egui / egui_plot, estética Avionics Vector)
```

### Reglas de Dependencias entre Crates
- `x56-proto` no debe depender de bibliotecas de SO ni de I/O. Debe mantenerse ligero y centrado en la serialización/deserialización de paquetes de bytes.
- `x56-hid` solo maneja la detección y transporte USB HID a través de `hidraw`. No contiene lógica de negocio de perfiles ni de curvas.
- `x56-core` centraliza la lógica de negocio y reutiliza `x56-proto` y `x56-hid`.
- `x56-cli` y `x56-gui` son consumidores de `x56-core` y no deben duplicar lógica de protocolo ni de perfiles.

---

## 3. Pautas de Código y Estándares Rust

- **Edición Rust**: Rust 2021 (`edition = "2021"`).
- **Gestión de Errores**:
  - En crates de biblioteca (`x56-proto`, `x56-hid`, `x56-core`): usar tipos de error fuertemente tipados con `thiserror`. Evitar `unwrap()` o `expect()` en rutas de ejecución en caliente o que interactúen con hardware/I/O.
  - En aplicaciones (`x56-cli`, `x56-gui`): usar `anyhow::Result` o propagación idiomática con mensajes de contexto enriquecidos.
- **Rendimiento e Hilos en la GUI**:
  - La GUI (`x56-gui`) debe mantener un renderizado fluido (30+ FPS).
  - Cualquier muestreo continuo o I/O de hardware (evdev, lectura continua de reportes HID) debe ejecutarse en hilos secundarios desacoplados mediante canales (`std::sync::mpsc` o `crossbeam-channel`), sin bloquear el bucle principal de `eframe`/`egui`.
- **Compatibilidad con Perfiles TOML**:
  - Mantener retrocompatibilidad en el esquema de perfiles TOML (`examples/default_profile.toml`).
  - Todo cambio en estructuras serializables debe considerar campos opcionales con `#[serde(default)]`.

---

## 4. Convenciones de Commits y Control de Versiones

- Seguir el estándar **Conventional Commits**:
  - `feat(<crate/área>): <descripción>`
  - `fix(<crate/área>): <descripción>`
  - `style(<crate/área>): <descripción>`
  - `refactor(<crate/área>): <descripción>`
  - `docs: <descripción>`
  - `test: <descripción>`
- Mantener los commits atómicos y descriptivos.

---

## 5. Verificación y Calidad de Código

Antes de dar por completado cualquier cambio o emitir un commit, validar los siguientes comandos:

1. **Compilación del workspace**:
   ```bash
   cargo check --workspace
   ```
2. **Suite de pruebas unitarias y de integración**:
   ```bash
   cargo test --workspace
   ```
3. **Linter Clippy**:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```
4. **Formato de código**:
   ```bash
   cargo fmt --all -- --check
   ```

---

## 6. Delegación y Coordinación con Herdr (`hc`)

Para tareas concurrentes, subagentes especializados o comandos pesados, se utiliza Herdr Core (`hc`) según la política definida en [.agents/rules/herdr-multimodel.md](.agents/rules/herdr-multimodel.md):

- **Modelos de Alto Razonamiento** (*Claude 3.7/3.5 Sonnet, GPT-4o/o3, Gemini Pro*): Usados para arquitectura, sincronización en `x56-gui`, matemática de curvas y reversing de paquetes HID en `x56-proto`.
- **Modelos Rápidos y Ligeros** (*Gemini Flash, GPT-4o-mini, Claude Haiku*): Usados para tests unitarios rutinarios, resolución mecánica de Clippy y documentación.
- **Auditores de Seguridad (Read-Only)**: Verificación estricta del aislamiento de la Interfaz 0 y ausencia de `unwrap()` en rutas críticas.
- **Procesos en Segundo Plano**: Compilaciones pesadas (`cargo build --release`) y suites de pruebas ejecutadas de forma no bloqueante con `hc run --bg`.
- **Gestión de Layout (Tabs y Panes)**: Si la tarea requiere visibilidad en tiempo real (p. ej. monitorear reportes HID/logs mientras se ejecuta una prueba o interactuar con un subagente visible), se pueden abrir nuevas pestañas (`herdr tab create` / `herdr-app do tab.new`) o dividir la pantalla (`herdr pane split --no-focus` / `herdr-app do pane.split`), preservando siempre el foco del usuario.


