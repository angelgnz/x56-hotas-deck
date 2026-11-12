---
description: Guía y política de delegación multi-modelo con Herdr (hc / herdr) para orquestar subagentes y comandos en segundo plano según la complejidad de la tarea.
trigger: model_decision
---

# Delegación Multi-Modelo con Herdr (`hc`)

Directrices para coordinar tareas concurrentes, subagentes especializados y comandos de larga duración utilizando Herdr / Herdr Core (`hc`), seleccionando el modelo de IA más adecuado para cada tipo de tarea.

---

## 1. Criterios de Selección de Modelos

| Perfil de Tarea | Tipo de Modelo Recomendado | Ejemplos de Modelos / Herramientas | Casos de Uso en x56linux |
|---|---|---|---|
| **Razonamiento Profundo y Arquitectura** | Modelos Frontier / High-Reasoning | Claude 3.7/3.5 Sonnet, GPT-4o / o1 / o3-mini, Gemini Pro | Ingeniería inversa de paquetes HID (Reportes 0x0B/0xFF), sincronización multihilo en `egui`, algoritmos de curvas matemáticas complejas. |
| **Ejecución Rápida y Mecánica** | Modelos Ligeros y Rápidos | Gemini Flash / Flash Lite, GPT-4o-mini, Claude Haiku | Pruebas unitarias repetitivas, corrección de advertencias de Clippy, actualización de docstrings, refactorizaciones sintácticas directas. |
| **Revisión de Código y Auditoría de Invariantes** | Agentes en Modo Read-Only / Auditor | Claude Sonnet / GPT-4o (con rol estricto de solo lectura) | Auditoría de aislamiento de Interfaz 0 (verificar que no se toque `/dev/input`), validación de `no_std` en `x56-proto`, revisión de `unwrap()` en rutas críticas. |
| **Procesos Pesados de Sistema** | Comandos Nativos (Sin LLM) | `hc run` / `cargo` | Compilación release (`cargo build --release`), suite de tests completa (`cargo test --workspace`), empaquetado (.deb/.rpm). |

---

## 2. Invariantes de Ejecución en Herdr

1. **Siempre en Segundo Plano (`--bg`)**:
   - Todo subagente o proceso lanzado con `hc` debe iniciarse con `--bg` para evitar bloquear la sesión principal y no contaminar la salida de terminal.
   - Usar `--name <nombre-descriptivo>` único para identificar el terminal.
2. **Definición Clara del Alcance y Reporte**:
   - Incluir la tarea inicial en el comando `run` o verificar disponibilidad con `hc wait <t>`.
   - Indicar explícitamente al subagente que reporte sus resultados vía:
     `hc send $HC_TERMINAL_ID "mensaje de resultado"`
   - Si la salida es extensa, instruir al subagente para que escriba en un archivo Markdown y devuelva únicamente la ruta del archivo.
3. **Restricción de Subdelegación Recursiva**:
   - Salvo autorización explícita, los subagentes NO deben instanciar subagentes adicionales.
4. **No Bloqueo / Sin Polling Activo**:
   - Nunca usar `sleep` en bucle. Usar `hc wait <t>` o `hc wait --children all/any`.
5. **Limpieza de Recursos**:
   - Una vez recopilados los resultados de una tarea completada, terminar y remover el terminal:
     `hc kill <nombre>; hc rm <nombre>`

---

## 3. Patrones de Comando con `hc`

### A. Subagente de Razonamiento Profundo (Arquitectura / Protocolo)
```bash
hc run --bg --name proto-arch --cwd "$PWD" claude \
  "Analiza la estructura de serialización en crates/x56-proto/src/lib.rs. No inicies subagentes. Reporta hallazgos con hc send \$HC_TERMINAL_ID cuando termines."
```

### B. Subagente Ligero / Rápido (Clippy / Tests)
```bash
hc run --bg --name test-fixer --cwd "$PWD" pi --provider google --model gemini-2.5-flash \
  "Revisa los warnings de cargo clippy en crates/x56-cli. Corrige sólo warnings menores. Reporta con hc send \$HC_TERMINAL_ID."
```

### C. Auditor de Seguridad / Invariantes (Read-Only)
```bash
hc run --bg --name security-audit --cwd "$PWD" claude \
  "Audita crates/x56-hid para confirmar que nunca se abre ni desconecta la Interfaz 0 de USB. Modo SOLO LECTURA, no modifiques ningún archivo. Reporta con hc send \$HC_TERMINAL_ID."
```

### D. Tarea Pesada de Compilación / Verificación (Sin Bloquear al Agente Principal)
```bash
hc run --bg --rm --no-notify --name test-suite cargo test --workspace
hc wait test-suite
hc read test-suite --lines 30
```

---

## 4. Gestión de Pestañas (Tabs) y División de Pantalla (Panes)

El agente puede crear nuevas pestañas o dividir la pantalla en paneles si la tarea lo requiere (p. ej. para mantener visible un monitor de logs/hidraw en vivo, inspeccionar telemetría concurrente, o ejecutar un subagente interactivo que el usuario deba supervisar en tiempo real).

### A. Criterios de Uso
- **Solo cuando la tarea lo justifique**: No crear pestañas ni paneles innecesarios si una tarea puede resolverse limpiamente en segundo plano con `hc run --bg`.
- **Preservar el Foco del Usuario**: Utilizar siempre `--no-focus` al dividir paneles o abrir terminales, de modo que el usuario no pierda el contexto activo.
- **Nombres Explícitos**: Nombrar siempre las pestañas con un identificador claro y en minúsculas (p. ej. `hidraw-live`, `cargo-watch`, `agent-reviewer`).
- **Geometría Ergonómica**: Dividir a la derecha (`right`) en pantallas anchas y hacia abajo (`down`) en paneles estrechos o alargados. Evitar divisiones recursivas que generen paneles demasiado pequeños.

### B. Comandos en `herdr` (Multiplexor CLI)
```bash
# Dividir el panel actual a la derecha conservando CWD y sin robar foco:
herdr pane split --current --direction right --cwd "$PWD" --no-focus

# Dividir el panel actual hacia abajo:
herdr pane split --current --direction down --cwd "$PWD" --no-focus

# Crear una nueva pestaña en el workspace actual:
herdr tab create --workspace "$HERDR_WORKSPACE_ID"
```

### C. Comandos en `herdr-app` (Entorno de Ventana / GUI)
```bash
# Crear una nueva pestaña con nombre y tarea descriptiva:
herdr-app do tab.new --name "diagnostico-x56" --task "Supervisión de hidraw y evdev"

# Dividir panel en la ventana activa:
herdr-app do pane.split --direction right --pane <pane-id>
```

