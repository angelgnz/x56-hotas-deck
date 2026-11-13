# Project Fluent - Localización en Español (es-ES) para x56linux

app-title = Centro de Control X-56 RHINO // RUST-EVDEV

# Subsistemas de Navegación
nav-curves = Curvas de Respuesta
nav-programming = Botones y Mapeo
nav-rgb = Iluminación RGB
nav-calibrate = Calibración Hall
nav-doctor = Diagnóstico

# Acciones y Badges de Barra Superior
header-profile = PERFIL
header-save-profile = GUARDAR
header-save-profile-tooltip = Guardar perfil activo en disco
header-load-profile = CARGAR
header-load-profile-tooltip = Abrir perfil TOML desde disco
header-test-panel = PRUEBA EN VIVO
header-test-panel-tooltip = Abrir panel de inspección de telemetría y mapeo de botones
header-stick = PALANCA
header-throttle = ACELERADOR
header-online = EN LÍNEA
header-offline = DESCONECTADO
header-uinput = DAEMON UINPUT

# Barra Lateral Táctica
sidebar-heading-devices = ESTADO DEL HARDWARE
sidebar-stick-title = Palanca (Stick)
sidebar-throttle-title = Acelerador (Throttle)
sidebar-spring-heading = TENSIÓN DEL MUELLE
sidebar-spring-none = Ninguno (Gimbal Libre)
sidebar-spring-blue = Azul (Suave / 1.5 kg)
sidebar-spring-yellow = Amarillo (Medio / 2.2 kg)
sidebar-spring-red = Rojo (Fuerte / 3.0 kg)
sidebar-system-heading = COMPORTAMIENTO DEL SISTEMA
sidebar-run-in-background = SEGUNDO PLANO AL CERRAR
sidebar-run-in-background-desc = Si está activado, cerrar la ventana mantendrá la aplicación y el mapeo uinput activos en la bandeja del sistema.
sidebar-language = IDIOMA
sidebar-lang-en = English (EN)
sidebar-lang-es = Español (ES)
sidebar-uinput-online = DAEMON UINPUT: EN LÍNEA
sidebar-uinput-offline = DAEMON UINPUT: DESCONECTADO
sidebar-uinput-online-tooltip = Teclado virtual /dev/uinput listo para inyección de teclas (incluidos juegos Proton/Steam).
sidebar-uinput-offline-tooltip = /dev/uinput no está disponible o carece de permisos de escritura. Revisa 70-x56.rules y el módulo uinput.
sidebar-proton-udev-ok = PROTON / UDEV: LISTO (70-x56)
sidebar-proton-udev-warn = PROTON / UDEV: PARCIAL (70-x56)
sidebar-proton-udev-fail = PROTON / UDEV: FALTA REGLA
sidebar-proton-udev-tooltip = { $detail } — Haz clic para abrir Diagnóstico del Sistema.

# Bandeja del Sistema y Menú Contextual
tray-toggle = Mostrar / Ocultar
tray-show = Mostrar Ventana
tray-hide = Ocultar Ventana
tray-quit = Salir de X56 HOTAS Deck
tray-profile = Perfil activo: { $name }
tray-uinput-online = ✓ Daemon Uinput: En línea
tray-uinput-offline = ✗ Daemon Uinput: Desconectado

# Pestaña de Curvas de Respuesta
curves-title = Respuesta de Curvas de Ejes y Ajuste de Zona Muerta
curves-transfer-title = RESPUESTA DE TRANSFERENCIA (CURVA)
curves-active-device = DISPOSITIVO ACTIVO:
curves-stick-rhino = PALANCA RHINO
curves-throttle-rhino = ACELERADOR RHINO
curves-select-axis = Seleccionar Eje
curves-axis-x = Alabeo (Eje X)
curves-axis-y = Cabeceo (Eje Y)
curves-axis-z = Guiñada (Timón / Twist)
curves-axis-tl = Acelerador Izquierdo
curves-axis-tr = Acelerador Derecho
curves-axis-r1 = Rotatorio 1
curves-axis-r2 = Rotatorio 2
curves-axis-r3 = Rotatorio 3
curves-axis-r4 = Rotatorio 4
curves-curvature = Curvatura (Perfil de Respuesta)
curves-deadband = Zona Muerta (Centro / Acelerador)
curves-saturation-x = Saturación X
curves-saturation-y = Y Saturación Y
curves-invert = Invertir Dirección
curves-live-telemetry = Telemetría de Eje en Vivo
curves-preset-linear = Lineal (1:1)
curves-preset-smooth = Curva S Suave
curves-preset-aggressive = Combate Agresivo
curves-preset-precision = Alta Precisión
curves-apply = Aplicar Curva al Hardware
curves-reset-btn = RESTABLECER (1:1 LINEAL)
curves-read-btn = LEER DE HARDWARE
curves-applied-success = ¡Configuración de curva enviada al hardware con éxito!
curves-status-gimbal-sync = Mini-Gimbal analógico sincronizado con pipeline uinput.
curves-status-eeprom-synced = EEPROM SINCRONIZADA // Configuración leída del hardware.
curves-status-sim-mode = Modo simulación activo ({ $device } no detectado en /dev/hidraw*).
curves-status-reset = Parámetros de curva revertidos a valores nominales.

# Pestaña de Iluminación RGB
rgb-title = Sistema de Iluminación LED RGB
rgb-active-hw-mode = MODO FÍSICO ACTIVO:
rgb-follow-hw = Seguir selector físico
rgb-follow-hw-tooltip = Cambia automáticamente el modo seleccionado en esta vista al girar la perilla M1/M2/S1 del Throttle
rgb-desc = Selecciona un modo (M1, M2, S1) para asignarle su propio color LED RGB. Al girar el selector físico del X56, la iluminación cambiará automáticamente al color guardado de ese modo.
rgb-target-device = DISPOSITIVO DESTINO:
rgb-target-both = AMBOS (STICK + THROTTLE)
rgb-target-stick = SOLO PALANCA
rgb-target-throttle = SOLO ACELERADOR
rgb-all-modes-btn = TODOS LOS MODOS (GLOBAL M1+M2+S1)
rgb-all-modes-tooltip = Aplicar el mismo color a los tres modos (M1, M2 y S1) simultáneamente
rgb-color-picker-heading = SELECTOR DE COLOR // { $mode }
rgb-red = Rojo  (R): { $val }
rgb-green = Verde (G): { $val }
rgb-blue = Azul  (B): { $val }
rgb-apply-to = APLICAR A { $mode }
rgb-apply-tooltip = Asignar este color al modo seleccionado y enviarlo inmediatamente al hardware USB
rgb-save-to-toml = GUARDAR EN PERFIL TOML
rgb-save-tooltip = Guardar los colores de todos los modos (M1, M2, S1) en el archivo de perfil TOML activo
rgb-presets-heading = PREAJUSTES TÁCTICOS // { $mode }
rgb-presets-desc = Haz clic en un color para asignarlo al modo seleccionado:
rgb-preset-cyan-rhino = Cyan Rhino (Fábrica)
rgb-preset-tactical-amber = Ámbar Táctico / Naranja
rgb-preset-night-combat = Rojo Combate Nocturno
rgb-preset-supercruise = Púrpura Supercrucero
rgb-preset-hud-green = Verde Esmeralda
rgb-preset-ops-blue = Azul Operaciones / S1
rgb-preset-pure-white = Blanco Puro
rgb-preset-off = Apagar Iluminación (Off)
rgb-quick-schemes = ESQUEMAS MULTIMODO RÁPIDOS (M1 + M2 + S1):
rgb-scheme-tactical = Esquema Táctico: M1 Púrpura | M2 Rojo | S1 Azul
rgb-scheme-cockpit = Esquema Cabina: M1 Verde | M2 Ámbar | S1 Cyan
rgb-select-device = Dispositivo Objetivo
rgb-both = Ambos Dispositivos (Palanca y Acelerador)
rgb-mode-m1 = Modo 1 (M1 - Navegación/Crucero)
rgb-mode-m2 = Modo 2 (M2 - Combate/Ataque)
rgb-mode-s1 = Modo 3 (S1 - Aterrizaje/Atraque)
rgb-mode-global = Todos los Modos (Global)
rgb-color-preview = Vista Previa de Color
rgb-preset-cyan = Cian Aviónica
rgb-preset-emerald = Verde HUD
rgb-preset-amber = Ámbar Advertencia
rgb-preset-red = Rojo Alerta
rgb-preset-white = Blanco Puro
rgb-apply = Aplicar RGB al Hardware
rgb-applied-success = ¡Configuración RGB enviada a Interfaz 2 con éxito!
rgb-status-applied = Color RGB { $hex } asignado a [{ $mode } // { $target }] y aplicado en { $count } dispositivo(s).
rgb-status-saved = Color { $hex } guardado en [{ $mode } // { $target }] (Aviso USB: { $err })

# Pestaña de Programación de Botones y Modos
prog-title = Programación de Botones y Mapeo de Modos
prog-hw-mode = MODO FÍSICO:
prog-router-uinput = ROUTER UINPUT:
prog-router-online = EN LÍNEA (1000 HZ)
prog-router-offline = SIN ACCESO /DEV/UINPUT
prog-device = DISPOSITIVO:
prog-device-stick = PALANCA (STICK)
prog-device-throttle = ACELERADOR (THROTTLE)
prog-search-placeholder = Buscar acción / botón...
prog-virtual-layers = CAPAS VIRTUALES
prog-modes-shift = 3 MODOS + SHIFT
prog-clear = LIMPIAR
prog-mode-selector = Selector de Modo Físico
prog-submode-sld = SLD (Aerofreno / Deslizador)
prog-bind-key = Asignar Tecla / Macro
prog-clear-bind = Limpiar Asignación
prog-click-node = Haz clic en un botón o cruceta en el esquema para inspeccionar o configurar el mapeo.
prog-assigned-keys = Teclas Asignadas
prog-uinput-status = Dispositivo virtual uinput: Activo (muestreo a 1000Hz)
prog-filter-all = TODOS
prog-filter-triggers = GATILLOS
prog-filter-hats = SETAS POV & ANALÓGICAS
prog-filter-face = BOTONES FRONTALES
prog-filter-macros = MACROS Y SECUENCIAS
prog-filter-shift = MODIFICADOR SHIFT
prog-mode-m1-desc = DOGFIGHT / COMBATE
prog-mode-m2-desc = NAVEGACIÓN CUÁNTICA
prog-mode-s1-desc = ATERRIZAJE / MINERÍA
prog-monitor-title = MONITOR DE DETECCIÓN EN VIVO (EVDEV HOOK)
prog-test-loop = TEST LOOP
prog-sync-hw-on = SYNC HW: ON
prog-sync-hw-off = SYNC HW: OFF
prog-sync-hw-tooltip = Sincroniza automáticamente la capa visible con el selector físico del Throttle

# Pestaña de Calibración Hall
cal-title = Calibración Neutral de Sensores de Efecto Hall
cal-subtitle = Recalibra el punto cero magnético de los ejes X (Roll) e Y (Pitch) de la palanca.
cal-instructions = Instrucciones de Seguridad:
cal-step1 = Paso 1: Centrar Palanca y Acelerador a Cero
cal-step2 = Paso 2: Muestrear Desviación de Centro Raw
cal-step3 = Paso 3: Escribir Calibración Neutral en EEPROM
cal-dry-run = Modo Seguro Dry-Run (Previsualizar Desviaciones)
cal-start = Iniciar Secuencia de Calibración
cal-success = ¡Calibración Hall guardada con éxito!
cal-sim-complete = Muestreo completado en modo simulación (ningún dato escrito en EEPROM).
cal-success-eeprom = ¡Calibración física guardada exitosamente en la EEPROM del Stick!

# Pestaña de Diagnóstico del Sistema (Doctor)
doc-title = Diagnóstico del Sistema e Integridad (x56 doctor)
doc-subtitle = Verifica que las reglas udev, permisos sin root y drivers del kernel estén listos.
doc-run-checks = Ejecutar Diagnóstico Completo
doc-refresh = Re-analizar Sistema
doc-status-ok = SISTEMA 100% OPERATIVO
doc-status-fail = SE REQUIEREN ACCIONES CORRECTIVAS
doc-status-warn = ADVERTENCIAS DETECTADAS
doc-summary = ({ $ok } OK, { $warn } advertencias, { $fail } fallos)
doc-check-udev = Reglas udev y Permisos uaccess
doc-check-hidraw = Acceso a Interfaz 2 de Proveedor
doc-check-joydev = Driver de Joystick del Kernel (/dev/input/js*)
doc-check-evdev = Aislamiento de Interfaz 0 evdev
doc-check-uinput = Daemon de Teclado Virtual Linux uinput
doc-check-firmware = Handshake de Microcontrolador Saitek/Logitech
doc-pass = CORRECTO
doc-fail = FALLIDO
doc-warn = ADVERTENCIA

# Cuadros de Diálogo y Notificaciones
dialog-load-title = Seleccionar Perfil TOML de X56
dialog-save-title = Guardar Perfil TOML de X56
profile-saved = ¡Perfil guardado en disco con éxito!
profile-loaded = ¡Perfil cargado con éxito!
filter-toml = Perfiles TOML (*.toml)
filter-all = Todos los archivos (*.*)
