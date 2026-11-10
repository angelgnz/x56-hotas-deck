//! Pestaña táctica de Programación y Mapeo de Botones ("Avionics Vector")
//! para el Saitek / Logitech G X56 HOTAS (Inspirada en la Sección 8 del Manual).
//!
//! Implementa la arquitectura visual Avionics Vector:
//! - Top Bar táctica con selector de dispositivo, pills M1/M2/S1 y estado del router uinput.
//! - Columna Izquierda: Monitor de detección en vivo (evdev hook 1000 Hz), mapa vectorial 2D interactivo,
//!   matriz POV Hat 1 con brújula octogonal y botones de acción rápida de macros.
//! - Columna Derecha: Buscador táctico, estadísticas telemétricas, chips de filtro y filas de mapeo
//!   con badges destacados uinput y metadatos de Star Citizen / Simuladores.
//! - Footer Táctico: Badge de compatibilidad Proton/Steam Deck y acciones globales de exportación y commit.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Key, RichText, Stroke, Ui, Vec2};
use x56_core::{
    catalog_for_device, ButtonDescriptor, ButtonMonitor, CommandCell, DeviceKind,
    LiveButtonsSnapshot, ModeSlot, ProgrammingProfile, TriggeredCommandEvent,
};

use crate::theme::{self, *};

/// Categorías de filtro táctico para el catálogo de botones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterCategory {
    #[default]
    Todos,
    Gatillos,
    Setas,
    BotonesFrontales,
    Macros,
    Shift,
}

impl FilterCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Todos => "TODOS",
            Self::Gatillos => "GATILLOS",
            Self::Setas => "SETAS POV & ANALÓGICAS",
            Self::BotonesFrontales => "BOTONES FRONTALES",
            Self::Macros => "MACROS Y SECUENCIAS",
            Self::Shift => "MODIFICADOR SHIFT",
        }
    }
}

/// Estado y lógica de la pestaña táctica de Programación de Botones y Modos.
pub struct ProgrammingTab {
    // Estado principal
    pub selected_device: DeviceKind,
    pub selected_mode: ModeSlot,
    pub follow_hardware_mode: bool,
    pub show_diagram_view: bool,
    pub profile: ProgrammingProfile,
    pub editing_button_id: Option<String>,
    pub pending_keys: Vec<String>,
    pub pending_name: String,
    pub hovered_button_id: Option<String>,
    pub show_test_window: bool,
    pub test_log: Vec<TriggeredCommandEvent>,
    pub filter_group: Option<String>,
    pub status_message: Option<(String, bool)>,
    pub test_input_text: String,

    // Avionics Vector Redesign
    pub search_query: String,
    pub filter_category: FilterCategory,
    pub last_detected_button: Option<String>,
    pub last_hex_code: String,
    pub last_latency_ms: f32,
    pub last_state_down: bool,
    pub is_recording_macro: bool,
    pub macro_record_buffer: Vec<String>,
    pub active_detected_id: Option<String>,
    pub prev_pressed_buttons: HashSet<String>,
    pub currently_pressed_count: usize,
    pub active_pressed_summary: String,
}

impl Default for ProgrammingTab {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgrammingTab {
    /// Inicializa la pestaña con los parámetros por defecto de Avionics Vector.
    pub fn new() -> Self {
        Self {
            selected_device: DeviceKind::Stick,
            selected_mode: ModeSlot::M1,
            follow_hardware_mode: true,
            show_diagram_view: true,
            profile: ProgrammingProfile::default(),
            editing_button_id: None,
            pending_keys: Vec::new(),
            pending_name: String::new(),
            hovered_button_id: None,
            show_test_window: false,
            test_log: Vec::new(),
            filter_group: None,
            status_message: None,
            test_input_text: String::new(),

            search_query: String::new(),
            filter_category: FilterCategory::Todos,
            last_detected_button: None,
            last_hex_code: "0x0120".to_string(),
            last_latency_ms: 0.82,
            last_state_down: false,
            is_recording_macro: false,
            macro_record_buffer: Vec::new(),
            active_detected_id: None,
            prev_pressed_buttons: HashSet::new(),
            currently_pressed_count: 0,
            active_pressed_summary: String::new(),
        }
    }

    /// Carga un perfil de programación y lo sincroniza con el monitor de botones.
    pub fn load_programming_profile(&mut self, prof: ProgrammingProfile, monitor: &ButtonMonitor) {
        self.profile = prof;
        monitor.set_profile(self.profile.clone());
        self.editing_button_id = None;
        self.pending_keys.clear();
        self.pending_name.clear();
        self.status_message = Some((
            "PERFIL TÁCTICO SINCRONIZADO CORRECTAMENTE // UINPUT LISTO".to_string(),
            false,
        ));
    }

    /// Devuelve una copia del perfil de programación actual.
    pub fn current_programming_profile(&self) -> ProgrammingProfile {
        self.profile.clone()
    }

    /// Renderizado principal de la pestaña de programación.
    pub fn ui(&mut self, ui: &mut Ui, monitor: &ButtonMonitor) {
        // Establecer el ancho máximo disponible para evitar cualquier desbordamiento horizontal
        let avail_w = ui.available_width();
        ui.set_max_width(avail_w);

        let snapshot = monitor.snapshot();

        // 1. Sincronización física de hardware si está activo el seguimiento automático
        if self.follow_hardware_mode {
            self.selected_mode = snapshot.active_mode;
        }

        // 2. Detección en vivo de pulsaciones físicas para telemetría
        self.update_live_detection_state(&snapshot);

        // 3. Captura interactiva de teclas cuando una celda está en edición
        self.handle_live_key_capture(ui);

        // 4. Barra Superior Táctica (Top Bar)
        self.render_tactical_top_bar(ui, monitor, &snapshot);

        ui.add_space(6.0);

        // Mensaje de estado táctico / toast si está presente
        let mut dismiss_status = false;
        if let Some((ref msg, is_err)) = self.status_message {
            let col = if is_err {
                COLOR_ERROR
            } else {
                COLOR_TERTIARY_CONTAINER
            };
            theme::sunken_frame().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("● TELEMETRÍA //")
                            .color(col)
                            .size(10.0)
                            .monospace()
                            .strong(),
                    );
                    ui.label(
                        RichText::new(msg)
                            .color(COLOR_PRIMARY)
                            .size(11.0)
                            .monospace(),
                    );
                    if ui.small_button("✖").clicked() {
                        dismiss_status = true;
                    }
                });
            });
            ui.add_space(4.0);
        }
        if dismiss_status {
            self.status_message = None;
        }

        // 5. Layout Táctico: 2 Columnas en pantallas amplias (>= 850px) o Apilado Responsivo (< 850px)
        let total_w = ui.available_width();

        if self.show_diagram_view {
            if total_w >= 850.0 {
                let gap = 12.0;
                let left_w = (total_w * 0.35).clamp(310.0, 390.0);
                let right_w = (total_w - left_w - gap - 6.0).max(260.0);

                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;

                    // ================================================================
                    // COLUMNA IZQUIERDA: Monitor, Mapa Vectorial 2D, POV Hat
                    // ================================================================
                    ui.allocate_ui_with_layout(
                        Vec2::new(left_w, ui.available_height() - 48.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("left_col_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    // A. Monitor de detección en vivo (EVDEV Hook)
                                    self.render_evdev_hook_card(ui);
                                    ui.add_space(8.0);

                                    // B. Mapa Vectorial de Entradas X-56 2D
                                    self.render_vector_map_card(ui, &snapshot);
                                    ui.add_space(8.0);

                                    // C. POV Hat 1 Live Matrix (Brújula octogonal)
                                    self.render_pov_compass_card(ui, &snapshot);
                                    ui.add_space(8.0);

                                    // D. Botones de acción rápida de macros
                                    self.render_quick_macro_actions(ui);
                                    ui.add_space(6.0);
                                });
                        },
                    );

                    // ================================================================
                    // COLUMNA DERECHA: Buscador, Estadísticas, Filtros y Mapeos
                    // ================================================================
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_w, ui.available_height() - 48.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            // A. Buscador táctico y estadísticas telemétricas
                            self.render_search_and_stats(ui);
                            ui.add_space(6.0);

                            // B. Pills de filtro por categoría
                            self.render_filter_pills(ui);
                            ui.add_space(6.0);

                            // C. Filas de mapeo tácticas en tarjetas
                            egui::ScrollArea::vertical()
                                .id_salt("mapping_rows_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.render_mapping_cards(ui, monitor, &snapshot);
                                });
                        },
                    );
                });
            } else {
                // Modo Apilado Responsivo para pantallas compactas (< 850px)
                egui::ScrollArea::vertical()
                    .id_salt("programming_stacked_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Monitor de detección en vivo (EVDEV Hook)
                        self.render_evdev_hook_card(ui);
                        ui.add_space(8.0);

                        // Mapa Vectorial de Entradas X-56 2D
                        self.render_vector_map_card(ui, &snapshot);
                        ui.add_space(8.0);

                        // POV Hat 1 Live Matrix (Brújula octogonal)
                        self.render_pov_compass_card(ui, &snapshot);
                        ui.add_space(8.0);

                        // Botones de acción rápida de macros
                        self.render_quick_macro_actions(ui);
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);

                        // Buscador táctico y estadísticas telemétricas
                        self.render_search_and_stats(ui);
                        ui.add_space(6.0);

                        // Pills de filtro por categoría
                        self.render_filter_pills(ui);
                        ui.add_space(6.0);

                        // Filas de mapeo tácticas en tarjetas
                        self.render_mapping_cards(ui, monitor, &snapshot);
                    });
            }
        } else {
            // Modo Solo Celdas (ancho completo)
            ui.allocate_ui_with_layout(
                Vec2::new(total_w, ui.available_height() - 48.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.render_search_and_stats(ui);
                    ui.add_space(6.0);
                    self.render_filter_pills(ui);
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("mapping_rows_scroll_full")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.render_mapping_cards(ui, monitor, &snapshot);
                        });
                },
            );
        }

        // 6. Footer Táctico con Compatibilidad y Acciones Globales
        self.render_tactical_footer(ui, monitor);

        // 7. Modal de prueba de loop evdev
        if self.show_test_window {
            self.render_test_modal(ui, monitor, &snapshot);
        }
    }

    /// Actualiza el estado del monitor telemétrico a partir de las entradas físicas.
    fn update_live_detection_state(&mut self, snapshot: &LiveButtonsSnapshot) {
        self.currently_pressed_count = snapshot.pressed_buttons.len();
        if !snapshot.pressed_buttons.is_empty() {
            let all_desc = x56_core::all_button_descriptors();
            let mut labels: Vec<String> = snapshot
                .pressed_buttons
                .iter()
                .map(|id| {
                    all_desc
                        .iter()
                        .find(|d| &d.id == id)
                        .map(|d| format!("• {} ({})", d.label, d.id.to_uppercase()))
                        .unwrap_or_else(|| format!("• {}", id.to_uppercase()))
                })
                .collect();
            labels.sort();
            self.active_pressed_summary = labels.join("\n");
        } else {
            self.active_pressed_summary.clear();
        }

        if snapshot.pressed_buttons.is_empty() {
            self.last_state_down = false;
            self.active_detected_id = None;
            self.prev_pressed_buttons.clear();
            return;
        }

        self.last_state_down = true;

        // 1. Detectar transiciones de nuevos botones presionados (0 -> 1)
        let mut newly_pressed: Vec<String> = snapshot
            .pressed_buttons
            .iter()
            .filter(|id| !self.prev_pressed_buttons.contains(*id))
            .cloned()
            .collect();

        if !newly_pressed.is_empty() {
            // Si hay botones recién presionados, orden determinista y priorizamos el nuevo botón
            newly_pressed.sort();
            self.active_detected_id = Some(newly_pressed[0].clone());
        } else if let Some(ref current) = self.active_detected_id {
            // Si el botón actual sigue presionado, lo mantenemos estable (sin flicker)
            if !snapshot.pressed_buttons.contains(current) {
                // El botón activo fue liberado; seleccionar de forma determinista entre los que sigan activos
                let mut remaining: Vec<String> = snapshot.pressed_buttons.iter().cloned().collect();
                remaining.sort();
                self.active_detected_id = Some(remaining[0].clone());
            }
        } else {
            // Caso inicial: sin botón previo pero con botones presionados
            let mut remaining: Vec<String> = snapshot.pressed_buttons.iter().cloned().collect();
            remaining.sort();
            self.active_detected_id = Some(remaining[0].clone());
        }

        self.prev_pressed_buttons = snapshot.pressed_buttons.clone();

        // 2. Actualizar telemetría visual del HUD con el botón activo
        if let Some(ref btn_id) = self.active_detected_id {
            let all_desc = x56_core::all_button_descriptors();
            if let Some(desc) = all_desc.iter().find(|d| &d.id == btn_id) {
                self.last_detected_button =
                    Some(format!("{} // {}", desc.id.to_uppercase(), desc.label));
                self.last_hex_code = if desc.evdev_code != 0 {
                    format!("{:#06x}", desc.evdev_code)
                } else {
                    "ABS_HAT0".to_string()
                };
                self.hovered_button_id = Some(desc.id.clone());
            } else {
                self.last_detected_button = Some(btn_id.to_uppercase());
                self.hovered_button_id = Some(btn_id.clone());
            }
        }
    }

    /// Captura pulsaciones de teclado en vivo para la celda en edición.
    fn handle_live_key_capture(&mut self, ui: &mut Ui) {
        if self.editing_button_id.is_none() {
            return;
        }

        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }

        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } = event
                {
                    if *key == Key::Backspace && !self.pending_keys.is_empty() {
                        self.pending_keys.pop();
                        continue;
                    }

                    if let Some(name) = map_key_to_name(*key) {
                        let name_str = name.to_string();
                        if !self.pending_keys.contains(&name_str) {
                            self.pending_keys.push(name_str);
                        }
                    }
                }
            }
        });
    }

    // ========================================================================
    // TOP BAR TÁCTICA
    // ========================================================================

    fn render_tactical_top_bar(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        theme::header_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Switcher de Dispositivo
                let is_stick = self.selected_device == DeviceKind::Stick;
                if tactical_button(ui, "🕹️ X-56 RHINO STICK (JOYSTICK)", is_stick, false).clicked()
                    && !is_stick
                {
                    self.selected_device = DeviceKind::Stick;
                    self.editing_button_id = None;
                    self.hovered_button_id = None;
                }

                let is_throttle = self.selected_device == DeviceKind::Throttle;
                if tactical_button(ui, "🚀 X-56 DUAL THROTTLE", is_throttle, false).clicked()
                    && !is_throttle
                {
                    self.selected_device = DeviceKind::Throttle;
                    self.editing_button_id = None;
                    self.hovered_button_id = None;
                }

                ui.separator();

                // Botón de Alternancia de Vistas (Dual 5:7 vs Solo Celdas)
                let view_label = if self.show_diagram_view {
                    "👁️ VISTAS: DUAL (5:7)"
                } else {
                    "👁️ VISTAS: SOLO CELDAS"
                };
                if tactical_button(ui, view_label, self.show_diagram_view, false)
                    .on_hover_text("Alterna entre la vista combinada con Esquema Vectorial y la vista de Solo Celdas")
                    .clicked()
                {
                    self.show_diagram_view = !self.show_diagram_view;
                }

                ui.separator();

                // Selector de Modo Físico
                ui.label(
                    RichText::new("MODO FÍSICO:")
                        .color(COLOR_OUTLINE)
                        .size(10.0)
                        .monospace()
                        .strong(),
                );

                let modes = [
                    (ModeSlot::M1, "DOGFIGHT / COMBATE", COLOR_PRIMARY_CONTAINER),
                    (ModeSlot::M2, "QUANTUM NAV", COLOR_WARNING_AMBER),
                    (ModeSlot::S1, "LANDING / MINING", COLOR_SECONDARY),
                ];

                for (slot, default_label, active_tint) in modes {
                    let is_sel = self.selected_mode == slot;
                    let bindings = self.profile.bindings_for(slot);
                    let label = if let Some(ref name) = bindings.name {
                        format!("{}: {}", slot.as_str(), name.to_uppercase())
                    } else {
                        format!("{}: {}", slot.as_str(), default_label)
                    };

                    let tint = bindings
                        .rgb_for(DeviceKind::Stick)
                        .and_then(|hex| x56_core::parse_color(hex).ok())
                        .map(|c| Color32::from_rgb(c.r, c.g, c.b))
                        .unwrap_or(active_tint);

                    let (bg, fg, stroke_col) = if is_sel {
                        (tint, Color32::WHITE, tint)
                    } else {
                        (COLOR_SURFACE_CONTAINER, COLOR_ON_SURFACE, COLOR_OUTLINE_VARIANT)
                    };

                    let btn = egui::Button::new(
                        RichText::new(label)
                            .size(10.5)
                            .color(fg)
                            .strong()
                            .monospace(),
                    )
                    .fill(bg)
                    .stroke(Stroke::new(1.0, stroke_col))
                    .corner_radius(egui::CornerRadius::same(4));

                    if ui.add(btn).clicked() {
                        self.selected_mode = slot;
                        self.editing_button_id = None;
                    }
                }

                // Checkbox Sincronización HW
                let hw_sync_label = if self.follow_hardware_mode {
                    "🔄 SYNC HW: ON"
                } else {
                    "🔄 SYNC HW: OFF"
                };
                let hw_sync_col = if self.follow_hardware_mode {
                    COLOR_TERTIARY_CONTAINER
                } else {
                    COLOR_OUTLINE
                };

                if ui
                    .button(
                        RichText::new(hw_sync_label)
                            .color(hw_sync_col)
                            .size(10.0)
                            .monospace()
                            .strong(),
                    )
                    .on_hover_text("Sincroniza automáticamente la capa visible con el selector físico del Throttle")
                    .clicked()
                {
                    self.follow_hardware_mode = !self.follow_hardware_mode;
                    if self.follow_hardware_mode {
                        self.selected_mode = snapshot.active_mode;
                    }
                }

                ui.separator();

                // Estado del Router uinput
                let uinput_ready = monitor.uinput_available();
                let (dot_col, router_text) = if uinput_ready {
                    (
                        COLOR_TERTIARY_CONTAINER,
                        "UINPUT ROUTER: ONLINE (4ms)",
                    )
                } else {
                    (
                        COLOR_ERROR,
                        "UINPUT: SOLO LECTURA",
                    )
                };

                theme::sunken_frame().inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("●").color(dot_col).size(11.0));
                        ui.label(
                            RichText::new(router_text)
                                .color(COLOR_PRIMARY_FIXED)
                                .size(10.0)
                                .monospace(),
                        );
                    });
                });

                // Botón TEST EVDEV LOOP
                if tactical_button(ui, "⚡ TEST LOOP", false, true).clicked() {
                    self.show_test_window = true;
                }
            });
        });
    }

    // ========================================================================
    // COLUMNA IZQUIERDA: TARJETA 1 - MONITOR DE DETECCIÓN EN VIVO
    // ========================================================================

    fn render_evdev_hook_card(&self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🛰️ MONITOR DE DETECCIÓN EN VIVO (EVDEV HOOK)")
                        .color(COLOR_PRIMARY)
                        .size(11.0)
                        .strong()
                        .monospace(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    telemetry_badge(ui, "HOOK", "LISTENING 1000Hz", COLOR_TERTIARY_CONTAINER);
                });
            });

            ui.add_space(4.0);

            // Fila: Evento detectado
            theme::sunken_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("EVENTO:")
                            .color(COLOR_OUTLINE)
                            .size(10.0)
                            .monospace()
                            .strong(),
                    );
                    let (ev_name, ev_col) = if let Some(ref det) = self.last_detected_button {
                        (det.as_str(), COLOR_PRIMARY_CONTAINER)
                    } else {
                        (
                            "REPOSO // LISTO PARA DETECCIÓN FÍSICA",
                            COLOR_ON_SURFACE_VARIANT,
                        )
                    };
                    ui.label(
                        RichText::new(ev_name)
                            .color(ev_col)
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );

                    if self.currently_pressed_count > 1 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let tip = format!(
                                "Entradas físicas simultáneas ({}):\n{}",
                                self.currently_pressed_count, self.active_pressed_summary
                            );
                            ui.label(
                                RichText::new(format!(
                                    "● {} ACTIVOS",
                                    self.currently_pressed_count
                                ))
                                .color(COLOR_WARNING_AMBER)
                                .size(9.5)
                                .monospace()
                                .strong(),
                            )
                            .on_hover_text(tip);
                        });
                    }
                });
            });

            ui.add_space(4.0);

            // 3 Tarjetas telemétricas en fila: HEX CODE, LATENCIA, ESTADO
            ui.columns(3, |cols| {
                // HEX CODE
                theme::sunken_frame().show(&mut cols[0], |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("HEX CODE")
                                .color(COLOR_OUTLINE)
                                .size(9.0)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(&self.last_hex_code)
                                .color(COLOR_PRIMARY_CONTAINER)
                                .size(13.0)
                                .strong()
                                .monospace(),
                        );
                    });
                });

                // LATENCIA
                theme::sunken_frame().show(&mut cols[1], |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("LATENCIA")
                                .color(COLOR_OUTLINE)
                                .size(9.0)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(format!("{:.2} ms", self.last_latency_ms))
                                .color(COLOR_TERTIARY_CONTAINER)
                                .size(13.0)
                                .strong()
                                .monospace(),
                        );
                    });
                });

                // ESTADO
                let (val_str, val_col) = if self.last_state_down {
                    ("VAL: 1 (DOWN)", COLOR_TERTIARY_CONTAINER)
                } else {
                    ("VAL: 0 (UP)", COLOR_ON_SURFACE_VARIANT)
                };
                theme::sunken_frame().show(&mut cols[2], |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("ESTADO")
                                .color(COLOR_OUTLINE)
                                .size(9.0)
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(val_str)
                                .color(val_col)
                                .size(11.5)
                                .strong()
                                .monospace(),
                        );
                    });
                });
            });
        });
    }

    // ========================================================================
    // COLUMNA IZQUIERDA: TARJETA 2 - MAPA VECTORIAL DE ENTRADAS 2D
    // ========================================================================

    fn render_vector_map_card(&mut self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        theme::card_frame().show(ui, |ui| {
            let title = match self.selected_device {
                DeviceKind::Stick => {
                    "MAPA VECTORIAL DE ENTRADAS // X-56 STICK (HOTAS RHINO RUST CORE)"
                }
                DeviceKind::Throttle => {
                    "MAPA VECTORIAL DE ENTRADAS // X-56 DUAL THROTTLE (RUST CORE)"
                }
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title)
                        .color(COLOR_PRIMARY)
                        .size(11.0)
                        .strong()
                        .monospace(),
                );
            });

            ui.add_space(4.0);

            // Canvas de dibujo vectorial con egui::Painter
            let canvas_height = (ui.available_width() * 0.72).clamp(280.0, 420.0);
            let canvas_size = Vec2::new(ui.available_width(), canvas_height);
            let (response, painter) = ui.allocate_painter(canvas_size, egui::Sense::click());
            let rect = response.rect;

            // Fondo sunken y rejilla táctica
            painter.rect_filled(rect, 6.0, COLOR_SURFACE_LOWEST);
            painter.rect_stroke(
                rect,
                6.0,
                Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
                egui::StrokeKind::Inside,
            );

            // Rejilla HUD sutil
            let step = 32.0;
            let mut x = rect.min.x + step;
            while x < rect.max.x {
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                    Stroke::new(0.5, COLOR_GRID_HUD),
                );
                x += step;
            }
            let mut y = rect.min.y + step;
            while y < rect.max.y {
                painter.line_segment(
                    [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                    Stroke::new(0.5, COLOR_GRID_HUD),
                );
                y += step;
            }

            // Escuadras tácticas de esquina HUD
            let corner_len = 10.0;
            let bracket_stroke = Stroke::new(1.5, COLOR_PRIMARY_CONTAINER);
            // Top-left
            painter.line_segment(
                [
                    rect.left_top(),
                    egui::pos2(rect.min.x + corner_len, rect.min.y),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    rect.left_top(),
                    egui::pos2(rect.min.x, rect.min.y + corner_len),
                ],
                bracket_stroke,
            );
            // Top-right
            painter.line_segment(
                [
                    rect.right_top(),
                    egui::pos2(rect.max.x - corner_len, rect.min.y),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    rect.right_top(),
                    egui::pos2(rect.max.x, rect.min.y + corner_len),
                ],
                bracket_stroke,
            );
            // Bottom-left
            painter.line_segment(
                [
                    rect.left_bottom(),
                    egui::pos2(rect.min.x + corner_len, rect.max.y),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    rect.left_bottom(),
                    egui::pos2(rect.min.x, rect.max.y - corner_len),
                ],
                bracket_stroke,
            );
            // Bottom-right
            painter.line_segment(
                [
                    rect.right_bottom(),
                    egui::pos2(rect.max.x - corner_len, rect.max.y),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    rect.right_bottom(),
                    egui::pos2(rect.max.x, rect.max.y - corner_len),
                ],
                bracket_stroke,
            );

            // Dibujar siluetas vectoriales de fondo
            match self.selected_device {
                DeviceKind::Stick => self.draw_vector_stick_silhouette(&painter, rect),
                DeviceKind::Throttle => self.draw_vector_throttle_silhouette(&painter, rect),
            }

            // Dibujar pines interactivos con resplandor
            let nodes = get_device_schematic_nodes(self.selected_device);
            let mouse_pos = ui.input(|i| i.pointer.hover_pos());
            let clicked = response.clicked();

            let mut newly_hovered: Option<String> = None;
            let mut clicked_pin: Option<String> = None;

            for node in &nodes {
                let center = egui::pos2(
                    rect.min.x + node.norm_x * rect.width(),
                    rect.min.y + node.norm_y * rect.height(),
                );

                let is_mouse_over = if let Some(mpos) = mouse_pos {
                    if node.is_rect {
                        let half = node.rect_size * 0.5;
                        mpos.x >= center.x - half.x
                            && mpos.x <= center.x + half.x
                            && mpos.y >= center.y - half.y
                            && mpos.y <= center.y + half.y
                    } else {
                        (mpos - center).length() <= node.radius + 4.0
                    }
                } else {
                    false
                };

                if is_mouse_over {
                    newly_hovered = Some(node.id.to_string());
                    if clicked {
                        clicked_pin = Some(node.id.to_string());
                    }
                }

                let is_pressed = snapshot.pressed_buttons.contains(node.id);
                let is_hovered =
                    self.hovered_button_id.as_deref() == Some(node.id) || is_mouse_over;
                let is_assigned = self
                    .profile
                    .get_cell(self.selected_mode, node.id)
                    .is_some_and(|c| c.is_assigned());

                // Determinación cromática Avionics Vector
                let (fill, border, glow_col, glow_radius) = if is_pressed {
                    (
                        COLOR_TERTIARY_CONTAINER,
                        Color32::WHITE,
                        COLOR_TERTIARY_CONTAINER,
                        node.radius + 6.0,
                    )
                } else if is_hovered {
                    (
                        COLOR_PRIMARY_CONTAINER,
                        Color32::WHITE,
                        COLOR_PRIMARY_CONTAINER,
                        node.radius + 5.0,
                    )
                } else if is_assigned {
                    (
                        COLOR_WARNING_AMBER,
                        Color32::from_rgb(255, 235, 170),
                        COLOR_WARNING_AMBER,
                        node.radius + 3.0,
                    )
                } else {
                    (
                        COLOR_SURFACE_CONTAINER,
                        COLOR_OUTLINE_VARIANT,
                        Color32::TRANSPARENT,
                        0.0,
                    )
                };

                // Resplandor táctico exterior
                if glow_radius > 0.0 {
                    painter.circle_stroke(
                        center,
                        glow_radius,
                        Stroke::new(
                            1.5,
                            Color32::from_rgba_premultiplied(
                                glow_col.r(),
                                glow_col.g(),
                                glow_col.b(),
                                70,
                            ),
                        ),
                    );
                }

                // Cuerpo del pin
                if node.is_rect {
                    let node_rect = egui::Rect::from_center_size(center, node.rect_size);
                    painter.rect_filled(node_rect, 3.0, fill);
                    painter.rect_stroke(
                        node_rect,
                        3.0,
                        Stroke::new(1.0, border),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    painter.circle_filled(center, node.radius, fill);
                    painter.circle_stroke(center, node.radius, Stroke::new(1.0, border));
                }

                // Etiqueta corta del nodo
                let text_col = if is_pressed || is_hovered || is_assigned {
                    COLOR_SURFACE_LOWEST
                } else {
                    COLOR_ON_SURFACE_VARIANT
                };

                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    node.short_label,
                    egui::FontId::monospace(9.0),
                    text_col,
                );

                // Tooltip táctico
                if is_mouse_over {
                    let default_info = default_mapping_for(node.id);
                    let mapped_str =
                        if let Some(cell) = self.profile.get_cell(self.selected_mode, node.id) {
                            if cell.is_assigned() {
                                format!("{} [{}]", cell.name, cell.keys_display())
                            } else {
                                format!(
                                    "DEFAULT: {} {}",
                                    default_info.key_display, default_info.action
                                )
                            }
                        } else {
                            format!(
                                "DEFAULT: {} {}",
                                default_info.key_display, default_info.action
                            )
                        };

                    response.clone().on_hover_ui_at_pointer(|ui| {
                        theme::sunken_frame().show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(node.full_name)
                                        .color(COLOR_PRIMARY)
                                        .size(12.0)
                                        .strong()
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "ID: {} // EVDEV: {}",
                                        node.id, node.short_label
                                    ))
                                    .color(COLOR_OUTLINE)
                                    .size(9.5)
                                    .monospace(),
                                );
                                ui.separator();
                                ui.label(
                                    RichText::new(mapped_str)
                                        .color(COLOR_PRIMARY_CONTAINER)
                                        .size(11.0)
                                        .strong()
                                        .monospace(),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "GRUPO: {} // TIPO: {}",
                                        default_info.group_sc, default_info.map_type
                                    ))
                                    .color(COLOR_ON_SURFACE_VARIANT)
                                    .size(9.0)
                                    .monospace(),
                                );
                            });
                        });
                    });
                }
            }

            if let Some(hovered) = newly_hovered {
                self.hovered_button_id = Some(hovered);
            }

            if let Some(pin_id) = clicked_pin {
                self.search_query = pin_id.clone();
                self.editing_button_id = Some(pin_id.clone());
                if let Some(cell) = self.profile.get_cell(self.selected_mode, &pin_id) {
                    self.pending_name = cell.name.clone();
                    self.pending_keys = cell.keys.clone();
                } else {
                    let def = default_mapping_for(&pin_id);
                    self.pending_name = def.action.to_string();
                    self.pending_keys = vec![def.key_display.replace(['[', ']'], "")];
                }
            }
        });
    }

    /// Dibuja la silueta técnica del Stick en el mapa vectorial según la distribución física.
    fn draw_vector_stick_silhouette(&self, painter: &egui::Painter, rect: egui::Rect) {
        let outline = Stroke::new(1.0, COLOR_OUTLINE_VARIANT);

        // Cabezal superior del Stick (enmarca B, Witch, A, POV y Castle)
        let head_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.14,
                rect.min.y + rect.height() * 0.04,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.86,
                rect.min.y + rect.height() * 0.38,
            ),
        );
        painter.rect_stroke(head_rect, 8.0, outline, egui::StrokeKind::Inside);

        // Biseles circulares de fondo para los 3 Hats (según diagrama de usuario)
        // 1. POV 8-Way (Cluster POV)
        let pov_c = egui::pos2(
            rect.min.x + rect.width() * 0.246,
            rect.min.y + rect.height() * 0.242,
        );
        painter.circle_stroke(pov_c, 24.0, Stroke::new(0.8, COLOR_OUTLINE_VARIANT));
        painter.text(
            pov_c,
            egui::Align2::CENTER_CENTER,
            "POV",
            egui::FontId::monospace(8.0),
            COLOR_OUTLINE,
        );

        // 2. Witch-Hat (HAT 1)
        let witch_c = egui::pos2(
            rect.min.x + rect.width() * 0.587,
            rect.min.y + rect.height() * 0.121,
        );
        painter.circle_stroke(witch_c, 24.0, Stroke::new(0.8, COLOR_OUTLINE_VARIANT));
        painter.text(
            witch_c,
            egui::Align2::CENTER_CENTER,
            "WITCH",
            egui::FontId::monospace(7.5),
            COLOR_OUTLINE,
        );

        // 3. Castle-Hat (HAT 2)
        let castle_c = egui::pos2(
            rect.min.x + rect.width() * 0.690,
            rect.min.y + rect.height() * 0.285,
        );
        painter.circle_stroke(castle_c, 24.0, Stroke::new(0.8, COLOR_OUTLINE_VARIANT));
        painter.text(
            castle_c,
            egui::Align2::CENTER_CENTER,
            "CASTLE",
            egui::FontId::monospace(7.5),
            COLOR_OUTLINE,
        );

        // Mástil y empuñadura ergonómica centrada (cubre Trigger, C, Pinky y D)
        let shaft_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.30,
                rect.min.y + rect.height() * 0.36,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.70,
                rect.min.y + rect.height() * 0.92,
            ),
        );
        painter.rect_stroke(shaft_rect, 6.0, outline, egui::StrokeKind::Inside);
    }

    /// Dibuja la silueta técnica del Throttle en el mapa vectorial.
    fn draw_vector_throttle_silhouette(&self, painter: &egui::Painter, rect: egui::Rect) {
        let outline = Stroke::new(1.0, COLOR_OUTLINE_VARIANT);
        let accent = Stroke::new(1.2, COLOR_PRIMARY_CONTAINER);

        // Doble cuadrante de gases
        let lever_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.14,
                rect.min.y + rect.height() * 0.08,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.86,
                rect.min.y + rect.height() * 0.46,
            ),
        );
        painter.rect_stroke(lever_rect, 6.0, outline, egui::StrokeKind::Inside);

        // Separador central de palancas gemelas
        painter.line_segment(
            [
                egui::pos2(
                    rect.min.x + rect.width() * 0.50,
                    rect.min.y + rect.height() * 0.08,
                ),
                egui::pos2(
                    rect.min.x + rect.width() * 0.50,
                    rect.min.y + rect.height() * 0.46,
                ),
            ],
            Stroke::new(1.0, COLOR_OUTLINE_VARIANT),
        );

        // Base del Throttle con bahía de conmutadores TGL 1-7
        let base_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.10,
                rect.min.y + rect.height() * 0.50,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.90,
                rect.min.y + rect.height() * 0.93,
            ),
        );
        painter.rect_stroke(base_rect, 6.0, accent, egui::StrokeKind::Inside);

        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.50,
                rect.min.y + rect.height() * 0.53,
            ),
            egui::Align2::CENTER_CENTER,
            "DUAL THROTTLE CONSOLE // TGL 1-7 BAY",
            egui::FontId::monospace(8.5),
            COLOR_OUTLINE,
        );
    }

    // ========================================================================
    // COLUMNA IZQUIERDA: TARJETA 3 - POV HAT 1 LIVE MATRIX
    // ========================================================================

    fn render_pov_compass_card(&self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        theme::card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🧭 POV HAT 1 LIVE MATRIX // 8-WAY COMPASS")
                        .color(COLOR_PRIMARY)
                        .size(11.0)
                        .strong()
                        .monospace(),
                );
            });

            ui.add_space(4.0);

            // Calcular dirección actual del POV
            let up = snapshot.pressed_buttons.contains("stick_pov_up");
            let right = snapshot.pressed_buttons.contains("stick_pov_right");
            let down = snapshot.pressed_buttons.contains("stick_pov_down");
            let left = snapshot.pressed_buttons.contains("stick_pov_left");

            let (angle_str, abs_x, abs_y, dir_idx) = match (up, right, down, left) {
                (true, true, _, _) => ("045° [NORDESTE - NE]", 1, -1, Some(1)),
                (true, _, _, true) => ("315° [NOROESTE - NW]", -1, -1, Some(7)),
                (true, _, _, _) => ("000° [NORTE - N]", 0, -1, Some(0)),
                (_, true, true, _) => ("135° [SURESTE - SE]", 1, 1, Some(3)),
                (_, _, true, true) => ("225° [SUROESTE - SW]", -1, 1, Some(5)),
                (_, _, true, _) => ("180° [SUR - S]", 0, 1, Some(4)),
                (_, true, _, _) => ("090° [ESTE - E]", 1, 0, Some(2)),
                (_, _, _, true) => ("270° [OESTE - W]", -1, 0, Some(6)),
                _ => ("CENTRO [RETÍCULA NEUTRA]", 0, 0, None),
            };

            ui.horizontal(|ui| {
                // Brújula octogonal renderizada
                let compass_size = Vec2::new(120.0, 110.0);
                let (_, painter) = ui.allocate_painter(compass_size, egui::Sense::hover());
                let c = painter.clip_rect().center();
                let r = 42.0;

                // Círculo base y retícula
                painter.circle_stroke(c, r, Stroke::new(1.0, COLOR_OUTLINE_VARIANT));
                painter.circle_stroke(c, 16.0, Stroke::new(1.0, COLOR_OUTLINE_VARIANT));
                painter.line_segment(
                    [egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)],
                    Stroke::new(0.5, COLOR_GRID_HUD),
                );
                painter.line_segment(
                    [egui::pos2(c.x, c.y - r), egui::pos2(c.x, c.y + r)],
                    Stroke::new(0.5, COLOR_GRID_HUD),
                );

                // 8 puntos de dirección octogonal
                let dir_names = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
                for (i, name) in dir_names.iter().enumerate() {
                    let rad: f32 =
                        (i as f32) * std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
                    let p = egui::pos2(c.x + r * rad.cos(), c.y + r * rad.sin());
                    let is_active_dir = dir_idx == Some(i);

                    let (col, dot_r) = if is_active_dir {
                        (COLOR_TERTIARY_CONTAINER, 4.0)
                    } else {
                        (COLOR_OUTLINE_VARIANT, 2.0)
                    };
                    painter.circle_filled(p, dot_r, col);

                    if is_active_dir {
                        // Vector hacia el punto activo
                        painter.line_segment([c, p], Stroke::new(2.0, COLOR_TERTIARY_CONTAINER));
                    }

                    let text_p =
                        egui::pos2(c.x + (r + 10.0) * rad.cos(), c.y + (r + 10.0) * rad.sin());
                    painter.text(
                        text_p,
                        egui::Align2::CENTER_CENTER,
                        *name,
                        egui::FontId::monospace(8.0),
                        if is_active_dir {
                            COLOR_TERTIARY_CONTAINER
                        } else {
                            COLOR_OUTLINE
                        },
                    );
                }

                // Centro activo o neutro
                if dir_idx.is_none() {
                    painter.circle_filled(c, 3.5, COLOR_PRIMARY_CONTAINER);
                }

                ui.add_space(8.0);

                // Telemetría de la seta
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("ÁNGULO ACTUAL:")
                            .color(COLOR_OUTLINE)
                            .size(9.5)
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(angle_str)
                            .color(if dir_idx.is_some() {
                                COLOR_TERTIARY_CONTAINER
                            } else {
                                COLOR_PRIMARY
                            })
                            .size(11.0)
                            .strong()
                            .monospace(),
                    );
                    ui.add_space(4.0);
                    telemetry_badge(
                        ui,
                        "ABS_HAT0X",
                        &format!("{:+}", abs_x),
                        COLOR_PRIMARY_CONTAINER,
                    );
                    ui.add_space(2.0);
                    telemetry_badge(
                        ui,
                        "ABS_HAT0Y",
                        &format!("{:+}", abs_y),
                        COLOR_PRIMARY_CONTAINER,
                    );
                });
            });
        });
    }

    // ========================================================================
    // COLUMNA IZQUIERDA: ACCIONES RÁPIDAS DE MACROS
    // ========================================================================

    fn render_quick_macro_actions(&mut self, ui: &mut Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(
                RichText::new("⚡ ACCIONES RÁPIDAS DE MACROS // EVDEV")
                    .color(COLOR_PRIMARY)
                    .size(11.0)
                    .strong()
                    .monospace(),
            );
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                if tactical_button(ui, "⚡ NUEVA MACRO", false, false).clicked() {
                    self.status_message = Some((
                        "ASISTENTE DE MACROS TEMPORIZADAS (TIMED COMBOS) INICIALIZADO".to_string(),
                        false,
                    ));
                }

                let rec_label = if self.is_recording_macro {
                    "⏹ DETENER GRABACIÓN"
                } else {
                    "⏺ GRABAR SECUENCIA"
                };
                if tactical_button(ui, rec_label, self.is_recording_macro, false).clicked() {
                    self.is_recording_macro = !self.is_recording_macro;
                    if self.is_recording_macro {
                        self.macro_record_buffer.clear();
                        self.status_message = Some((
                            "GRABANDO SECUENCIA EVDEV EN VIVO // PULSA CUALQUIER BOTÓN".to_string(),
                            false,
                        ));
                    } else {
                        self.status_message = Some((
                            format!(
                                "SECUENCIA FINALIZADA ({} EVENTOS CAPTURADOS)",
                                self.macro_record_buffer.len()
                            ),
                            false,
                        ));
                    }
                }

                if tactical_button(ui, "🔄 PINKIE SHIFT (x24)", false, false).clicked() {
                    self.filter_category = FilterCategory::Shift;
                    self.status_message = Some((
                        "FILTRANDO CAPA DE MODIFICADOR PINKIE SHIFT (24 ASIGNACIONES)".to_string(),
                        false,
                    ));
                }
            });
        });
    }

    // ========================================================================
    // COLUMNA DERECHA: BUSCADOR TÁCTICO Y ESTADÍSTICAS
    // ========================================================================

    fn render_search_and_stats(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            // Buscador táctico
            theme::sunken_frame()
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍").size(12.0));
                        let search_w = (ui.available_width() - 40.0).clamp(140.0, 240.0);
                        let edit = egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Buscar botón, código evdev...")
                            .desired_width(search_w);
                        ui.add(edit);
                        if !self.search_query.is_empty() && ui.small_button("✖").clicked() {
                            self.search_query.clear();
                        }
                    });
                });

            ui.add_space(6.0);

            // Estadísticas telemétricas
            let catalog = catalog_for_device(self.selected_device);
            let total_buttons = catalog.len();
            let assigned_count = catalog
                .iter()
                .filter(|b| {
                    self.profile
                        .get_cell(self.selected_mode, &b.id)
                        .is_some_and(|c| c.is_assigned())
                })
                .count();

            telemetry_badge(
                ui,
                "BOTONES MAPEADOS",
                &format!("{} / {} ASIGNADOS", assigned_count, total_buttons),
                COLOR_TERTIARY_CONTAINER,
            );

            telemetry_badge(
                ui,
                "CAPAS VIRTUALES",
                "3 MODOS + SHIFT",
                COLOR_PRIMARY_CONTAINER,
            );
        });
    }

    // ========================================================================
    // COLUMNA DERECHA: PILLS DE FILTRO POR CATEGORÍA
    // ========================================================================

    fn render_filter_pills(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let categories = [
                FilterCategory::Todos,
                FilterCategory::Gatillos,
                FilterCategory::Setas,
                FilterCategory::BotonesFrontales,
                FilterCategory::Macros,
                FilterCategory::Shift,
            ];

            for cat in categories {
                let is_sel = self.filter_category == cat;
                let (bg, fg, stroke) = if is_sel {
                    (
                        COLOR_PRIMARY_CONTAINER,
                        COLOR_ON_PRIMARY_CONTAINER,
                        COLOR_PRIMARY_CONTAINER,
                    )
                } else {
                    (
                        COLOR_SURFACE_CONTAINER,
                        COLOR_ON_SURFACE,
                        COLOR_OUTLINE_VARIANT,
                    )
                };

                let btn = egui::Button::new(
                    RichText::new(cat.label())
                        .size(10.0)
                        .color(fg)
                        .strong()
                        .monospace(),
                )
                .fill(bg)
                .stroke(Stroke::new(1.0, stroke))
                .corner_radius(egui::CornerRadius::same(4));

                if ui.add(btn).clicked() {
                    self.filter_category = cat;
                    self.filter_group = None;
                }
            }

            if let Some(ref grp) = self.filter_group {
                ui.separator();
                let grp_badge = egui::Button::new(
                    RichText::new(format!("GRUPO: {} ✖", grp))
                        .size(10.0)
                        .color(COLOR_PRIMARY_CONTAINER)
                        .strong()
                        .monospace(),
                )
                .fill(COLOR_SURFACE_CONTAINER)
                .stroke(Stroke::new(1.0, COLOR_PRIMARY_CONTAINER))
                .corner_radius(egui::CornerRadius::same(4));

                if ui.add(grp_badge).clicked() {
                    self.filter_group = None;
                }
            }
        });
    }

    // ========================================================================
    // COLUMNA DERECHA: FILAS DE MAPEO TÁCTICAS
    // ========================================================================

    fn render_mapping_cards(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let catalog = catalog_for_device(self.selected_device);
        let q = self.search_query.trim().to_lowercase();

        let mut displayed_count = 0;

        for btn in &catalog {
            // Filtrado por grupo del manual si está activo
            if let Some(ref grp) = self.filter_group {
                if !btn.group.eq_ignore_ascii_case(grp) {
                    continue;
                }
            }
            // Filtrado por buscador táctico
            if !q.is_empty() {
                let matches_id = btn.id.to_lowercase().contains(&q);
                let matches_label = btn.label.to_lowercase().contains(&q);
                let matches_code = format!("{:#06x}", btn.evdev_code).contains(&q);
                let matches_assigned = self
                    .profile
                    .get_cell(self.selected_mode, &btn.id)
                    .is_some_and(|c| {
                        c.name.to_lowercase().contains(&q)
                            || c.keys_display().to_lowercase().contains(&q)
                    });

                if !matches_id && !matches_label && !matches_code && !matches_assigned {
                    continue;
                }
            }

            // Filtrado por categoría
            if !self.matches_category(&btn.id) {
                continue;
            }

            displayed_count += 1;
            self.render_tactical_mapping_row(ui, monitor, snapshot, btn);
            ui.add_space(4.0);
        }

        if displayed_count == 0 {
            theme::card_frame().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(
                        RichText::new("NO SE ENCONTRARON ENTRADAS PARA EL FILTRO ACTUAL")
                            .color(COLOR_OUTLINE)
                            .size(11.0)
                            .monospace(),
                    );
                    ui.add_space(20.0);
                });
            });
        }
    }

    /// Comprueba si un ID de botón pertenece a la categoría seleccionada.
    fn matches_category(&self, id: &str) -> bool {
        match self.filter_category {
            FilterCategory::Todos => true,
            FilterCategory::Gatillos => id.contains("trigger") || id.contains("pinkie"),
            FilterCategory::Setas => {
                id.contains("pov")
                    || id.contains("h1")
                    || id.contains("h2")
                    || id.contains("h3")
                    || id.contains("h4")
                    || id.contains("ministick")
                    || id == "stick_btn_c"
            }
            FilterCategory::BotonesFrontales => {
                id.contains("btn")
                    || id.contains("sw")
                    || id.contains("rty")
                    || id.contains("k1")
                    || id.contains("sld")
                    || id.contains("scroll")
            }
            FilterCategory::Macros => {
                if let Some(cell) = self.profile.get_cell(self.selected_mode, id) {
                    cell.keys.len() > 1
                } else {
                    false
                }
            }
            FilterCategory::Shift => id.contains("pinkie") || id.contains("flying"),
        }
    }

    /// Renderiza una fila individual de mapeo en tarjeta táctica.
    fn render_tactical_mapping_row(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
        btn: &ButtonDescriptor,
    ) {
        let is_physically_pressed = snapshot.pressed_buttons.contains(&btn.id);
        let is_editing = self.editing_button_id.as_deref() == Some(&btn.id);
        let is_hovered = self.hovered_button_id.as_deref() == Some(&btn.id);

        let cell_opt = self.profile.get_cell(self.selected_mode, &btn.id).cloned();
        let default_info = default_mapping_for(&btn.id);

        // Borde reactivo según el estado
        let border_stroke = if is_editing {
            Stroke::new(1.5, COLOR_PRIMARY_CONTAINER)
        } else if is_physically_pressed {
            Stroke::new(2.0, COLOR_TERTIARY_CONTAINER)
        } else if is_hovered {
            Stroke::new(1.2, COLOR_PRIMARY_FIXED)
        } else {
            Stroke::new(1.0, COLOR_OUTLINE_VARIANT)
        };

        let card_bg = if is_physically_pressed {
            Color32::from_rgb(0x13, 0x24, 0x20)
        } else {
            COLOR_SURFACE_LOW
        };

        let frame_resp = egui::Frame::new()
            .fill(card_bg)
            .stroke(border_stroke)
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                if is_editing {
                    self.render_inline_edit_controls(ui, monitor, btn);
                } else {
                    let card_w = ui.available_width();

                    // Badge de mapeo uinput destacado con fondo azul o cian
                    let (mapped_text, mapped_bg, mapped_fg) = if let Some(ref c) = cell_opt {
                        if c.is_assigned() {
                            (
                                format!("[{}] {}", c.keys_display(), c.name.to_uppercase()),
                                COLOR_SECONDARY_CONTAINER,
                                COLOR_ON_SECONDARY_CONTAINER,
                            )
                        } else {
                            (
                                format!("{} {}", default_info.key_display, default_info.action),
                                COLOR_SURFACE_HIGH,
                                COLOR_ON_SURFACE_VARIANT,
                            )
                        }
                    } else {
                        (
                            format!("{} {}", default_info.key_display, default_info.action),
                            COLOR_SURFACE_HIGH,
                            COLOR_ON_SURFACE_VARIANT,
                        )
                    };

                    // Fila 1: Identificador del control (Icono + Nombre descriptivo + Badge EVDEV + Estado físico)
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(default_info.icon).size(14.0));
                        ui.label(
                            RichText::new(btn.label)
                                .color(COLOR_PRIMARY)
                                .size(11.5)
                                .strong()
                                .monospace(),
                        );

                        let ev_str = if btn.evdev_code != 0 {
                            format!("{:#06x}", btn.evdev_code)
                        } else {
                            "ABS_HAT0".to_string()
                        };
                        telemetry_badge(ui, "EVDEV", &ev_str, COLOR_PRIMARY_FIXED);

                        if is_physically_pressed {
                            ui.label(
                                RichText::new("● PULSADO (VAL: 1)")
                                    .color(COLOR_TERTIARY_CONTAINER)
                                    .size(10.0)
                                    .strong()
                                    .monospace(),
                            );
                        }
                    });

                    ui.add_space(3.0);

                    // Fila 2: Asignación actual destacada (Badge uinput con comando y combinación de teclas)
                    ui.horizontal_wrapped(|ui| {
                        egui::Frame::new()
                            .fill(mapped_bg)
                            .stroke(Stroke::new(1.0, COLOR_OUTLINE_VARIANT))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&mapped_text)
                                        .color(mapped_fg)
                                        .size(10.5)
                                        .strong()
                                        .monospace(),
                                );
                            });
                    });

                    ui.add_space(4.0);

                    // Fila 3: Metadatos a la izquierda y botones de acción (Test, Editar, Limpiar)
                    // bajados responsivamente para que nunca se desborden ni queden cortados
                    if card_w >= 420.0 {
                        ui.horizontal(|ui| {
                            self.render_row_metadata(ui, &cell_opt, &default_info);

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    self.render_row_actions(ui, monitor, btn, &cell_opt, true);
                                },
                            );
                        });
                    } else {
                        // En anchos ultra compactos (< 420px), metadatos en Fila 3 y acciones en Fila 4
                        self.render_row_metadata(ui, &cell_opt, &default_info);
                        ui.add_space(3.0);
                        ui.horizontal_wrapped(|ui| {
                            self.render_row_actions_left_to_right(ui, monitor, btn, &cell_opt);
                        });
                    }
                }
            });

        if frame_resp.response.hovered() {
            self.hovered_button_id = Some(btn.id.clone());
        }
    }

    /// Sublínea con información de tipo, grupo y secuencia de macros.
    fn render_row_metadata(
        &self,
        ui: &mut Ui,
        cell_opt: &Option<CommandCell>,
        default_info: &DefaultMappingInfo,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "TIPO: {} // GRUPO: {}",
                    default_info.map_type, default_info.group_sc
                ))
                .color(COLOR_OUTLINE)
                .size(9.0)
                .monospace(),
            );
            if let Some(ref c) = cell_opt {
                if c.keys.len() > 1 {
                    ui.label(
                        RichText::new(format!("// SECUENCIA x{}", c.keys.len()))
                            .color(COLOR_WARNING_AMBER)
                            .size(9.0)
                            .monospace(),
                    );
                }
            }
        });
    }

    /// Botones de acción para cada fila (ordenados de derecha a izquierda).
    fn render_row_actions(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        btn: &ButtonDescriptor,
        cell_opt: &Option<CommandCell>,
        is_dropped_down: bool,
    ) {
        // En right_to_left, el primer elemento añadido queda más a la derecha.
        // 1. Limpiar
        if cell_opt.as_ref().is_some_and(|c| c.is_assigned()) {
            let clean_label = if is_dropped_down {
                "🗑 LIMPIAR"
            } else {
                "🗑"
            };
            if ui
                .button(
                    RichText::new(clean_label)
                        .color(COLOR_ERROR)
                        .size(10.0)
                        .monospace(),
                )
                .on_hover_text("Limpiar asignación uinput de este botón")
                .clicked()
            {
                self.profile.remove_cell(self.selected_mode, &btn.id);
                monitor.set_profile(self.profile.clone());
                self.status_message =
                    Some((format!("ASIGNACIÓN ELIMINADA // {}", btn.label), false));
            }
            ui.add_space(3.0);
        }

        // 2. Editar
        let edit_label = if is_dropped_down {
            "✏ EDITAR"
        } else {
            "EDITAR"
        };
        if tactical_button(ui, edit_label, false, false)
            .on_hover_text("Editar asignación de teclas y comando")
            .clicked()
        {
            self.editing_button_id = Some(btn.id.clone());
            if let Some(ref c) = cell_opt {
                self.pending_name = c.name.clone();
                self.pending_keys = c.keys.clone();
            } else {
                let default_info = default_mapping_for(&btn.id);
                self.pending_name = default_info.action.to_string();
                self.pending_keys = vec![default_info.key_display.replace(['[', ']'], "")];
            }
        }

        ui.add_space(3.0);

        // 3. Test
        let test_label = if is_dropped_down { "⚡ TEST" } else { "TEST" };
        if ui
            .button(
                RichText::new(test_label)
                    .color(COLOR_TERTIARY_CONTAINER)
                    .size(10.0)
                    .monospace(),
            )
            .on_hover_text("Simular pulsación para verificar uinput")
            .clicked()
        {
            self.last_detected_button = Some(format!("{} // {}", btn.id.to_uppercase(), btn.label));
            self.last_hex_code = if btn.evdev_code != 0 {
                format!("{:#06x}", btn.evdev_code)
            } else {
                "0x0120".to_string()
            };
            self.last_state_down = true;
            self.status_message = Some((format!("TEST SIMULADO: {}", btn.label), false));
        }
    }

    /// Botones de acción de izquierda a derecha para anchos muy reducidos (< 400px).
    fn render_row_actions_left_to_right(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        btn: &ButtonDescriptor,
        cell_opt: &Option<CommandCell>,
    ) {
        // Test
        if ui
            .button(
                RichText::new("⚡ TEST")
                    .color(COLOR_TERTIARY_CONTAINER)
                    .size(10.0)
                    .monospace(),
            )
            .on_hover_text("Simular pulsación para verificar uinput")
            .clicked()
        {
            self.last_detected_button = Some(format!("{} // {}", btn.id.to_uppercase(), btn.label));
            self.last_hex_code = if btn.evdev_code != 0 {
                format!("{:#06x}", btn.evdev_code)
            } else {
                "0x0120".to_string()
            };
            self.last_state_down = true;
            self.status_message = Some((format!("TEST SIMULADO: {}", btn.label), false));
        }

        ui.add_space(4.0);

        // Editar
        if tactical_button(ui, "✏ EDITAR", false, false)
            .on_hover_text("Editar asignación de teclas y comando")
            .clicked()
        {
            self.editing_button_id = Some(btn.id.clone());
            if let Some(ref c) = cell_opt {
                self.pending_name = c.name.clone();
                self.pending_keys = c.keys.clone();
            } else {
                let default_info = default_mapping_for(&btn.id);
                self.pending_name = default_info.action.to_string();
                self.pending_keys = vec![default_info.key_display.replace(['[', ']'], "")];
            }
        }

        if cell_opt.as_ref().is_some_and(|c| c.is_assigned()) {
            ui.add_space(4.0);
            if ui
                .button(
                    RichText::new("🗑 LIMPIAR")
                        .color(COLOR_ERROR)
                        .size(10.0)
                        .monospace(),
                )
                .on_hover_text("Limpiar asignación uinput de este botón")
                .clicked()
            {
                self.profile.remove_cell(self.selected_mode, &btn.id);
                monitor.set_profile(self.profile.clone());
                self.status_message =
                    Some((format!("ASIGNACIÓN ELIMINADA // {}", btn.label), false));
            }
        }
    }

    /// Controles de edición en línea de teclas y nombre de comando responsivos.
    fn render_inline_edit_controls(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        btn: &ButtonDescriptor,
    ) {
        let card_w = ui.available_width();
        ui.vertical(|ui| {
            let time = ui.input(|i| i.time);
            let cursor = if (time * 2.0).fract() < 0.5 {
                "▌"
            } else {
                " "
            };

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("EDITANDO: [{}] {}", btn.code_label, btn.label))
                        .color(COLOR_PRIMARY_CONTAINER)
                        .size(11.5)
                        .strong()
                        .monospace(),
                );
                ui.label(
                    RichText::new(format!("{cursor} ESPERANDO PULSACIÓN DE TECLADO..."))
                        .color(COLOR_WARNING_AMBER)
                        .size(10.5)
                        .monospace(),
                );
            });

            ui.add_space(4.0);

            // Mosaicos visuales de teclas capturadas
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("TECLAS:")
                        .color(COLOR_OUTLINE)
                        .size(10.0)
                        .monospace(),
                );
                if self.pending_keys.is_empty() {
                    ui.label(
                        RichText::new("(Presiona cualquier tecla física para capturar)")
                            .color(COLOR_ON_SURFACE_VARIANT)
                            .size(10.5)
                            .italics()
                            .monospace(),
                    );
                } else {
                    for key in &self.pending_keys {
                        egui::Frame::new()
                            .fill(COLOR_PRIMARY)
                            .corner_radius(egui::CornerRadius::same(3))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(key)
                                        .color(COLOR_SURFACE_LOWEST)
                                        .size(11.0)
                                        .strong()
                                        .monospace(),
                                );
                            });
                    }
                    if ui.small_button("Limpiar").clicked() {
                        self.pending_keys.clear();
                    }
                }
            });

            ui.add_space(4.0);

            // Campo de nombre y botones de confirmación responsivos
            if card_w >= 480.0 {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ACCIÓN:")
                            .color(COLOR_OUTLINE)
                            .size(10.0)
                            .monospace(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pending_name)
                            .desired_width((card_w * 0.35).clamp(120.0, 180.0))
                            .hint_text("Nombre de comando"),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                RichText::new("✖ CANCELAR")
                                    .color(COLOR_ERROR)
                                    .size(10.5)
                                    .monospace(),
                            )
                            .clicked()
                        {
                            self.editing_button_id = None;
                            self.pending_keys.clear();
                            self.pending_name.clear();
                        }

                        if tactical_button(ui, "✔ CONFIRMAR MAPEO", false, true).clicked() {
                            let name_final = if self.pending_name.trim().is_empty() {
                                btn.label.to_string()
                            } else {
                                self.pending_name.trim().to_string()
                            };

                            let cell = CommandCell {
                                button_id: btn.id.clone(),
                                name: name_final,
                                keys: self.pending_keys.clone(),
                            };

                            self.profile.set_cell(self.selected_mode, cell);
                            monitor.set_profile(self.profile.clone());
                            self.status_message =
                                Some((format!("MAPEO CONFIRMADO PARA {}", btn.label), false));

                            self.editing_button_id = None;
                            self.pending_keys.clear();
                            self.pending_name.clear();
                        }
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ACCIÓN:")
                            .color(COLOR_OUTLINE)
                            .size(10.0)
                            .monospace(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pending_name)
                            .desired_width(ui.available_width() - 8.0)
                            .hint_text("Nombre de comando"),
                    );
                });

                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    if tactical_button(ui, "✔ CONFIRMAR MAPEO", false, true).clicked() {
                        let name_final = if self.pending_name.trim().is_empty() {
                            btn.label.to_string()
                        } else {
                            self.pending_name.trim().to_string()
                        };

                        let cell = CommandCell {
                            button_id: btn.id.clone(),
                            name: name_final,
                            keys: self.pending_keys.clone(),
                        };

                        self.profile.set_cell(self.selected_mode, cell);
                        monitor.set_profile(self.profile.clone());
                        self.status_message =
                            Some((format!("MAPEO CONFIRMADO PARA {}", btn.label), false));

                        self.editing_button_id = None;
                        self.pending_keys.clear();
                        self.pending_name.clear();
                    }

                    if ui
                        .button(
                            RichText::new("✖ CANCELAR")
                                .color(COLOR_ERROR)
                                .size(10.5)
                                .monospace(),
                        )
                        .clicked()
                    {
                        self.editing_button_id = None;
                        self.pending_keys.clear();
                        self.pending_name.clear();
                    }
                });
            }
        });
    }

    // ========================================================================
    // FOOTER TÁCTICO
    // ========================================================================

    fn render_tactical_footer(&mut self, ui: &mut Ui, monitor: &ButtonMonitor) {
        theme::dock_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Badge de compatibilidad Proton & Steam Deck
                theme::sunken_frame()
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🛡️").size(12.0));
                            ui.label(
                                RichText::new("PROTON & STEAM DECK (udev: 99-logitech-x56.rules)")
                                    .color(COLOR_TERTIARY_CONTAINER)
                                    .size(9.5)
                                    .monospace(),
                            );
                        });
                    });

                ui.add_space(4.0);

                // Botones de exportación
                if tactical_button(ui, "📄 EXPORTAR XML", false, false).clicked() {
                    let xml = generate_star_citizen_xml(&self.profile);
                    if let Err(e) = std::fs::write("examples/star_citizen_actionmaps.xml", &xml) {
                        self.status_message = Some((format!("Error exportando XML: {e}"), true));
                    } else {
                        self.status_message = Some((
                            "MAPEO EXPORTADO: examples/star_citizen_actionmaps.xml".to_string(),
                            false,
                        ));
                    }
                }

                if tactical_button(ui, "💾 EXPORTAR JSON", false, false).clicked() {
                    let json = export_programming_json(&self.profile);
                    if let Err(e) = std::fs::write("examples/x56_buttons_config.json", &json) {
                        self.status_message = Some((format!("Error exportando JSON: {e}"), true));
                    } else {
                        self.status_message = Some((
                            "CONFIGURACIÓN EXPORTADA: examples/x56_buttons_config.json".to_string(),
                            false,
                        ));
                    }
                }

                ui.separator();

                // Descartar cambios
                if tactical_button(ui, "✖ DESCARTAR CAMBIOS", false, false).clicked() {
                    self.profile = ProgrammingProfile::default();
                    monitor.set_profile(self.profile.clone());
                    self.editing_button_id = None;
                    self.pending_keys.clear();
                    self.pending_name.clear();
                    self.status_message = Some((
                        "CAMBIOS DESCARTADOS // PERFIL REINICIADO".to_string(),
                        false,
                    ));
                }

                // Botón primario azul con resplandor
                if tactical_button(ui, "⚡ ESCRIBIR MAPEO A UINPUT", false, true).clicked() {
                    monitor.set_profile(self.profile.clone());
                    monitor.set_active(true);
                    self.status_message = Some((
                        "MAPEO ESCRITO A /dev/uinput // ROUTER ACTIVO".to_string(),
                        false,
                    ));
                }
            });
        });
    }

    // ========================================================================
    // MODAL DE PRUEBA DE LOOP EVDEV
    // ========================================================================

    fn render_test_modal(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let new_events = monitor.drain_events();
        self.test_log.extend(new_events);
        if self.test_log.len() > 300 {
            self.test_log.drain(0..self.test_log.len() - 300);
        }

        let mut close_modal = false;
        let mut open = self.show_test_window;

        egui::Window::new("⚡ TEST EVDEV LOOP // MONITOR EN TIEMPO REAL")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(720.0, 480.0))
            .show(ui.ctx(), |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("CAPTURA DE COMANDOS DISPARADOS Y RETORNO UINPUT")
                                .color(COLOR_PRIMARY)
                                .size(11.0)
                                .monospace()
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(
                                    RichText::new("🗑 LIMPIAR LOG")
                                        .color(COLOR_ERROR)
                                        .size(10.0)
                                        .monospace(),
                                )
                                .clicked()
                            {
                                self.test_log.clear();
                            }
                        });
                    });

                    ui.add_space(4.0);

                    // Indicadores
                    ui.horizontal(|ui| {
                        telemetry_badge(
                            ui,
                            "MODO FÍSICO",
                            snapshot.active_mode.as_str(),
                            COLOR_PRIMARY_CONTAINER,
                        );
                        let (u_txt, u_col) = if monitor.is_active() {
                            ("EMITIENDO (DIANA ON)", COLOR_TERTIARY_CONTAINER)
                        } else {
                            ("EN PAUSA (DIANA OFF)", COLOR_WARNING_AMBER)
                        };
                        telemetry_badge(ui, "ROUTER", u_txt, u_col);
                    });

                    ui.add_space(6.0);

                    // Campo de prueba de pulsaciones
                    ui.label(
                        RichText::new(
                            "CAMPO DE PRUEBA DE PULSACIONES (HAZ CLIC Y PULSA UN BOTÓN FÍSICO):",
                        )
                        .color(COLOR_OUTLINE)
                        .size(9.5)
                        .monospace(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.test_input_text).hint_text(
                            "El texto o atajos inyectados por uinput se escribirán aquí...",
                        ),
                    );

                    ui.add_space(6.0);
                    ui.separator();

                    // Tabla de eventos
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.test_log.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(20.0);
                                    ui.label(
                                        RichText::new("ESPERANDO EVENTOS EVDEV EN VIVO...")
                                            .color(COLOR_OUTLINE)
                                            .size(11.0)
                                            .monospace(),
                                    );
                                    ui.add_space(20.0);
                                });
                            } else {
                                egui::Grid::new("test_events_grid")
                                    .striped(true)
                                    .spacing(Vec2::new(12.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new("HORA")
                                                .color(COLOR_OUTLINE)
                                                .size(9.5)
                                                .monospace(),
                                        );
                                        ui.label(
                                            RichText::new("MODO")
                                                .color(COLOR_OUTLINE)
                                                .size(9.5)
                                                .monospace(),
                                        );
                                        ui.label(
                                            RichText::new("BOTÓN")
                                                .color(COLOR_OUTLINE)
                                                .size(9.5)
                                                .monospace(),
                                        );
                                        ui.label(
                                            RichText::new("COMANDO")
                                                .color(COLOR_OUTLINE)
                                                .size(9.5)
                                                .monospace(),
                                        );
                                        ui.label(
                                            RichText::new("TECLAS")
                                                .color(COLOR_OUTLINE)
                                                .size(9.5)
                                                .monospace(),
                                        );
                                        ui.label(
                                            RichText::new("ROUTER")
                                                .color(COLOR_OUTLINE)
                                                .size(9.5)
                                                .monospace(),
                                        );
                                        ui.end_row();

                                        for ev in &self.test_log {
                                            ui.label(
                                                RichText::new(&ev.timestamp_hms)
                                                    .color(COLOR_ON_SURFACE_VARIANT)
                                                    .size(9.5)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(ev.mode.as_str())
                                                    .color(COLOR_PRIMARY_CONTAINER)
                                                    .size(9.5)
                                                    .monospace()
                                                    .strong(),
                                            );
                                            ui.label(
                                                RichText::new(&ev.button_label)
                                                    .color(COLOR_PRIMARY)
                                                    .size(9.5)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(&ev.command_name)
                                                    .color(COLOR_TERTIARY_CONTAINER)
                                                    .size(9.5)
                                                    .monospace()
                                                    .strong(),
                                            );
                                            ui.label(
                                                RichText::new(ev.keys.join("+"))
                                                    .color(COLOR_PRIMARY_FIXED)
                                                    .size(9.5)
                                                    .monospace(),
                                            );
                                            let st = if ev.uinput_emitted {
                                                "✔ EMITIDO"
                                            } else {
                                                "ℹ SIMULADO"
                                            };
                                            ui.label(
                                                RichText::new(st)
                                                    .color(COLOR_TERTIARY_CONTAINER)
                                                    .size(9.5)
                                                    .monospace(),
                                            );
                                            ui.end_row();
                                        }
                                    });
                            }
                        });

                    ui.add_space(6.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tactical_button(ui, "CERRAR", false, false).clicked() {
                            close_modal = true;
                        }
                    });
                });
            });

        if !open || close_modal {
            self.show_test_window = false;
        }
    }
}

// ============================================================================
// HELPERS DE METADATOS TÁCTICOS Y MAPEADO PREDETERMINADO
// ============================================================================

struct DefaultMappingInfo {
    action: &'static str,
    key_display: &'static str,
    map_type: &'static str,
    group_sc: &'static str,
    icon: &'static str,
}

fn default_mapping_for(id: &str) -> DefaultMappingInfo {
    match id {
        "stick_trigger" => DefaultMappingInfo {
            action: "DISPARO PRIMARIO (WEAPONS)",
            key_display: "[ESPACIO]",
            map_type: "HOLD",
            group_sc: "SC: WEAPONS",
            icon: "🎯",
        },
        "stick_btn_a" => DefaultMappingInfo {
            action: "FIJAR BLANCO RETÍCULA (LOCK)",
            key_display: "[T]",
            map_type: "INSTANTÁNEO",
            group_sc: "SC: TARGETING",
            icon: "🎯",
        },
        "stick_btn_b" => DefaultMappingInfo {
            action: "CONTRAMEDIDAS // FLARE",
            key_display: "[R]",
            map_type: "TOGGLE (delay 120ms)",
            group_sc: "SC: SYSTEMS",
            icon: "🛡️",
        },
        "stick_btn_c" => DefaultMappingInfo {
            action: "C-STICK SENSOR SLEW PUSH",
            key_display: "[F3]",
            map_type: "INSTANTÁNEO",
            group_sc: "SC: SENSORS",
            icon: "🕹️",
        },
        "stick_btn_d" => DefaultMappingInfo {
            action: "DESACOPLAR VECTOR (DECOUPLED)",
            key_display: "[V]",
            map_type: "TOGGLE",
            group_sc: "SC: FLIGHT",
            icon: "🚀",
        },
        "stick_pinkie" => DefaultMappingInfo {
            action: "BOOST // POSTQUEMADOR",
            key_display: "[LSHIFT]",
            map_type: "HOLD",
            group_sc: "SC: FLIGHT",
            icon: "⚡",
        },
        "stick_pov_up" | "stick_pov_right" | "stick_pov_down" | "stick_pov_left" => {
            DefaultMappingInfo {
                action: "VISTA COCKPIT POV // HEAD LOOK",
                key_display: "[POV-LOOK]",
                map_type: "ANALOG/POV",
                group_sc: "SC: VIEW",
                icon: "🧭",
            }
        }
        "stick_h1_up" | "stick_h1_right" | "stick_h1_down" | "stick_h1_left" => {
            DefaultMappingInfo {
                action: "GESTIÓN BLANCOS RADAR // SUBSISTEMAS",
                key_display: "[P/O/I/U]",
                map_type: "INSTANTÁNEO",
                group_sc: "SC: RADAR",
                icon: "📡",
            }
        }
        "stick_h2_up" | "stick_h2_right" | "stick_h2_down" | "stick_h2_left" => {
            DefaultMappingInfo {
                action: "DISTRIBUCIÓN ENERGÍA ESCUDOS",
                key_display: "[F5..F8]",
                map_type: "INSTANTÁNEO",
                group_sc: "SC: POWER",
                icon: "⚡",
            }
        }
        "thr_sw_1" => DefaultMappingInfo {
            action: "DESPLIEGUE TREN ATERRIZAJE (TGL 1)",
            key_display: "[G]",
            map_type: "TOGGLE (anti-accidental)",
            group_sc: "SC: FLIGHT",
            icon: "🛬",
        },
        "thr_sw_2" => DefaultMappingInfo {
            action: "RECOGIDA TREN ATERRIZAJE (TGL 1)",
            key_display: "[LALT+G]",
            map_type: "TOGGLE (anti-accidental)",
            group_sc: "SC: FLIGHT",
            icon: "🛫",
        },
        "thr_sld" => DefaultMappingInfo {
            action: "AEROFRENOS // SPOILERS (SLD)",
            key_display: "[B]",
            map_type: "HOLD / POSITIONAL",
            group_sc: "SC: FLIGHT",
            icon: "🛑",
        },
        "thr_btn_e" => DefaultMappingInfo {
            action: "SALTO CUÁNTICO (QUANTUM TRAVEL)",
            key_display: "[B/LALT+B]",
            map_type: "HOLD (spool)",
            group_sc: "SC: NAVIGATION",
            icon: "🌌",
        },
        "thr_btn_h" => DefaultMappingInfo {
            action: "MODO MINERÍA // ESCANEO PING",
            key_display: "[M / TAB]",
            map_type: "TOGGLE",
            group_sc: "SC: MINING",
            icon: "⛏️",
        },
        "thr_btn_i" => DefaultMappingInfo {
            action: "LUCES EXTERIORES COCKPIT",
            key_display: "[L]",
            map_type: "TOGGLE",
            group_sc: "SC: SYSTEMS",
            icon: "💡",
        },
        "thr_ministick_push" => DefaultMappingInfo {
            action: "PROPULSORES LATERALES // STRAFE PUSH",
            key_display: "[LCTRL]",
            map_type: "INSTANTÁNEO",
            group_sc: "SC: STRAFE",
            icon: "🕹️",
        },
        "thr_rty_f_push" => DefaultMappingInfo {
            action: "LIMITADOR DE VELOCIDAD PUSH",
            key_display: "[BACKSPACE]",
            map_type: "TOGGLE",
            group_sc: "SC: PROPULSION",
            icon: "⚙️",
        },
        "thr_rty_g_push" => DefaultMappingInfo {
            action: "LÁSER DE EXTRACCIÓN PUSH",
            key_display: "[MOUSE1]",
            map_type: "HOLD",
            group_sc: "SC: MINING",
            icon: "⚙️",
        },
        "thr_scroll_fwd" => DefaultMappingInfo {
            action: "INCREMENTAR LIMITADOR ACELERACIÓN",
            key_display: "[SCROLL-UP]",
            map_type: "INSTANTÁNEO",
            group_sc: "SC: PROPULSION",
            icon: "⏫",
        },
        "thr_scroll_bck" => DefaultMappingInfo {
            action: "DISMINUIR LIMITADOR ACELERACIÓN",
            key_display: "[SCROLL-DN]",
            map_type: "INSTANTÁNEO",
            group_sc: "SC: PROPULSION",
            icon: "⏬",
        },
        "thr_k1_up" => DefaultMappingInfo {
            action: "MODO CRUCERO VELOCIDAD (SCM)",
            key_display: "[C]",
            map_type: "TOGGLE",
            group_sc: "SC: FLIGHT",
            icon: "🚀",
        },
        "thr_k1_down" => DefaultMappingInfo {
            action: "MODO PRECISIÓN MANIOBRA",
            key_display: "[LALT+C]",
            map_type: "TOGGLE",
            group_sc: "SC: FLIGHT",
            icon: "🎯",
        },
        _ => DefaultMappingInfo {
            action: "ACCIÓN ASIGNABLE HOTAS",
            key_display: "[DIRECT]",
            map_type: "INSTANTÁNEO",
            group_sc: "SC: GENERAL",
            icon: "🔘",
        },
    }
}

/// Mapea una tecla de egui a un nombre de tecla canónico de Linux / uinput.
fn map_key_to_name(key: Key) -> Option<&'static str> {
    match key {
        Key::ControlLeft => Some("Ctrl"),
        Key::ControlRight => Some("RCtrl"),
        Key::ShiftLeft => Some("Shift"),
        Key::ShiftRight => Some("RShift"),
        Key::AltLeft => Some("Alt"),
        Key::AltRight => Some("AltGr"),
        Key::Escape => Some("Escape"),
        Key::Tab => Some("Tab"),
        Key::Backspace => Some("Backspace"),
        Key::Enter => Some("Enter"),
        Key::Space => Some("Space"),
        Key::ArrowUp => Some("Up"),
        Key::ArrowDown => Some("Down"),
        Key::ArrowLeft => Some("Left"),
        Key::ArrowRight => Some("Right"),
        Key::Delete => Some("Delete"),
        Key::Insert => Some("Insert"),
        Key::Home => Some("Home"),
        Key::End => Some("End"),
        Key::PageUp => Some("PageUp"),
        Key::PageDown => Some("PageDown"),
        Key::A => Some("A"),
        Key::B => Some("B"),
        Key::C => Some("C"),
        Key::D => Some("D"),
        Key::E => Some("E"),
        Key::F => Some("F"),
        Key::G => Some("G"),
        Key::H => Some("H"),
        Key::I => Some("I"),
        Key::J => Some("J"),
        Key::K => Some("K"),
        Key::L => Some("L"),
        Key::M => Some("M"),
        Key::N => Some("N"),
        Key::O => Some("O"),
        Key::P => Some("P"),
        Key::Q => Some("Q"),
        Key::R => Some("R"),
        Key::S => Some("S"),
        Key::T => Some("T"),
        Key::U => Some("U"),
        Key::V => Some("V"),
        Key::W => Some("W"),
        Key::X => Some("X"),
        Key::Y => Some("Y"),
        Key::Z => Some("Z"),
        Key::Num0 => Some("0"),
        Key::Num1 => Some("1"),
        Key::Num2 => Some("2"),
        Key::Num3 => Some("3"),
        Key::Num4 => Some("4"),
        Key::Num5 => Some("5"),
        Key::Num6 => Some("6"),
        Key::Num7 => Some("7"),
        Key::Num8 => Some("8"),
        Key::Num9 => Some("9"),
        Key::F1 => Some("F1"),
        Key::F2 => Some("F2"),
        Key::F3 => Some("F3"),
        Key::F4 => Some("F4"),
        Key::F5 => Some("F5"),
        Key::F6 => Some("F6"),
        Key::F7 => Some("F7"),
        Key::F8 => Some("F8"),
        Key::F9 => Some("F9"),
        Key::F10 => Some("F10"),
        Key::F11 => Some("F11"),
        Key::F12 => Some("F12"),
        Key::Minus => Some("-"),
        Key::Equals => Some("="),
        Key::OpenBracket => Some("["),
        Key::CloseBracket => Some("]"),
        Key::Semicolon => Some(";"),
        Key::Quote => Some("'"),
        Key::Comma => Some(","),
        Key::Period => Some("."),
        Key::Slash => Some("/"),
        Key::Backslash => Some("\\"),
        Key::Backtick => Some("`"),
        _ => None,
    }
}

// ============================================================================
// EXPORTACIÓN STAR CITIZEN XML Y JSON
// ============================================================================

fn generate_star_citizen_xml(profile: &ProgrammingProfile) -> String {
    let mut xml = String::from("<!-- Star Citizen ActionMaps X-56 HOTAS (Avionics Vector) -->\n");
    xml.push_str("<ActionMaps version=\"1\" profileName=\"x56linux\">\n");
    xml.push_str("  <CustomisationUIHeader label=\"Logitech G X56 HOTAS (Rust Avionics Vector)\" description=\"Perfil táctico generado por x56-gui\">\n");
    xml.push_str("    <devices>\n");
    xml.push_str("      <joystick instance=\"1\"/>\n");
    xml.push_str("      <joystick instance=\"2\"/>\n");
    xml.push_str("    </devices>\n");
    xml.push_str("  </CustomisationUIHeader>\n");

    for (mode_idx, mode) in [ModeSlot::M1, ModeSlot::M2, ModeSlot::S1]
        .iter()
        .enumerate()
    {
        xml.push_str(&format!("  <!-- MODO FÍSICO {} -->\n", mode.as_str()));
        let bindings = profile.bindings_for(*mode);
        for (id, cell) in &bindings.cells {
            if cell.is_assigned() {
                xml.push_str(&format!(
                    "  <actionmap name=\"spaceship_general_m{}\">\n    <action name=\"{}\">\n      <rebind device=\"joystick\" input=\"{}\" key=\"{}\"/>\n    </action>\n  </actionmap>\n",
                    mode_idx + 1,
                    cell.name.replace(' ', "_").to_lowercase(),
                    id,
                    cell.keys.join("+")
                ));
            }
        }
    }

    xml.push_str("</ActionMaps>\n");
    xml
}

fn export_programming_json(profile: &ProgrammingProfile) -> String {
    let mut json = String::from("{\n  \"schema\": \"x56linux_programming_v1\",\n");
    for (mode_name, bindings) in [
        ("m1", &profile.m1),
        ("m2", &profile.m2),
        ("s1", &profile.s1),
    ] {
        json.push_str(&format!("  \"{}\": {{\n", mode_name));
        let mut first = true;
        for (id, cell) in &bindings.cells {
            if !first {
                json.push_str(",\n");
            }
            first = false;
            json.push_str(&format!(
                "    \"{}\": {{ \"name\": \"{}\", \"keys\": [{}] }}",
                id,
                cell.name,
                cell.keys
                    .iter()
                    .map(|k| format!("\"{}\"", k))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        json.push_str("\n  },\n");
    }
    json.push_str("  \"total_assigned\": ");
    json.push_str(&profile.total_assigned_count().to_string());
    json.push_str("\n}\n");
    json
}

// ============================================================================
// NODOS ESQUEMÁTICOS PARA EL MAPA VECTORIAL
// ============================================================================

struct SchematicNode {
    id: &'static str,
    short_label: &'static str,
    full_name: &'static str,
    norm_x: f32,
    norm_y: f32,
    radius: f32,
    is_rect: bool,
    rect_size: Vec2,
}

fn get_device_schematic_nodes(kind: DeviceKind) -> Vec<SchematicNode> {
    match kind {
        DeviceKind::Stick => vec![
            // 1. Sección A: POV 8-Way (Cluster POV: cx = 0.246, cy = 0.242)
            SchematicNode {
                id: "stick_pov_up",
                short_label: "P▲",
                full_name: "POV Vista Arriba",
                norm_x: 0.246,
                norm_y: 0.198,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_right",
                short_label: "P▶",
                full_name: "POV Vista Derecha",
                norm_x: 0.292,
                norm_y: 0.242,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_down",
                short_label: "P▼",
                full_name: "POV Vista Abajo",
                norm_x: 0.246,
                norm_y: 0.286,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_left",
                short_label: "P◀",
                full_name: "POV Vista Izquierda",
                norm_x: 0.200,
                norm_y: 0.242,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 2. Sección D: Botón A (Cabezal Superior) (cx = 0.351, cy = 0.138)
            SchematicNode {
                id: "stick_btn_a",
                short_label: "A",
                full_name: "Botón A (Cabezal Superior)",
                norm_x: 0.351,
                norm_y: 0.138,
                radius: 11.5,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 3. Sección C: HAT 1 (Witches) (Cluster Witch-Hat: cx = 0.587, cy = 0.121)
            SchematicNode {
                id: "stick_h1_up",
                short_label: "1▲",
                full_name: "HAT 1 (Witches) Arriba",
                norm_x: 0.587,
                norm_y: 0.077,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_right",
                short_label: "1▶",
                full_name: "HAT 1 (Witches) Derecha",
                norm_x: 0.633,
                norm_y: 0.121,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_down",
                short_label: "1▼",
                full_name: "HAT 1 (Witches) Abajo",
                norm_x: 0.587,
                norm_y: 0.165,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_left",
                short_label: "1◀",
                full_name: "HAT 1 (Witches) Izquierda",
                norm_x: 0.541,
                norm_y: 0.121,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 4. Sección D: Botón B (Lateral Pulgar) (cx = 0.781, cy = 0.072)
            SchematicNode {
                id: "stick_btn_b",
                short_label: "B",
                full_name: "Botón B (Lateral Pulgar)",
                norm_x: 0.781,
                norm_y: 0.072,
                radius: 11.5,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 5. Sección C: HAT 2 (Castle) (Cluster Castle-Hat: cx = 0.690, cy = 0.285)
            SchematicNode {
                id: "stick_h2_up",
                short_label: "2▲",
                full_name: "HAT 2 (Castle) Arriba",
                norm_x: 0.690,
                norm_y: 0.241,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_right",
                short_label: "2▶",
                full_name: "HAT 2 (Castle) Derecha",
                norm_x: 0.736,
                norm_y: 0.285,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_down",
                short_label: "2▼",
                full_name: "HAT 2 (Castle) Abajo",
                norm_x: 0.690,
                norm_y: 0.329,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_left",
                short_label: "2◀",
                full_name: "HAT 2 (Castle) Izquierda",
                norm_x: 0.644,
                norm_y: 0.285,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 6. Sección D: Gatillo Principal (Trigger: cx = 0.500, cy = 0.420)
            SchematicNode {
                id: "stick_trigger",
                short_label: "TRG",
                full_name: "Gatillo Principal (Trigger)",
                norm_x: 0.500,
                norm_y: 0.420,
                radius: 12.0,
                is_rect: true,
                rect_size: Vec2::new(34.0, 18.0),
            },
            // 7. Sección B: Botón C (Minipalanca Push: cx = 0.400, cy = 0.560)
            SchematicNode {
                id: "stick_btn_c",
                short_label: "C",
                full_name: "Botón C (Minipalanca Push)",
                norm_x: 0.400,
                norm_y: 0.560,
                radius: 11.5,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 8. Sección F: Palanca Meñique (Pinkie Lever: cx = 0.500, cy = 0.700)
            SchematicNode {
                id: "stick_pinkie",
                short_label: "PNK",
                full_name: "Palanca Meñique (Pinkie Lever)",
                norm_x: 0.500,
                norm_y: 0.700,
                radius: 11.5,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // 9. Sección D: Botón D (Meñique: cx = 0.600, cy = 0.840)
            SchematicNode {
                id: "stick_btn_d",
                short_label: "D",
                full_name: "Botón D (Meñique)",
                norm_x: 0.600,
                norm_y: 0.840,
                radius: 11.5,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
        ],
        DeviceKind::Throttle => vec![
            // Sección A: Giratorios F/G y Rueda Scroll
            SchematicNode {
                id: "thr_rty_f_push",
                short_label: "RTY F",
                full_name: "Giratorio F Push (RTY 1)",
                norm_x: 0.22,
                norm_y: 0.13,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_rty_g_push",
                short_label: "RTY G",
                full_name: "Giratorio G Push (RTY 2)",
                norm_x: 0.22,
                norm_y: 0.21,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_scroll_fwd",
                short_label: "SC▲",
                full_name: "Rueda Scroll Adelante",
                norm_x: 0.22,
                norm_y: 0.28,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_scroll_bck",
                short_label: "SC▼",
                full_name: "Rueda Scroll Atrás",
                norm_x: 0.22,
                norm_y: 0.35,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección B: Control Deslizante (SLD)
            SchematicNode {
                id: "thr_sld",
                short_label: "SLD",
                full_name: "Control Deslizante SLD (2 Posiciones)",
                norm_x: 0.78,
                norm_y: 0.13,
                radius: 11.0,
                is_rect: true,
                rect_size: Vec2::new(32.0, 16.0),
            },
            // Sección C: Botones Pulgar E, H, I, Minipalanca y K1
            SchematicNode {
                id: "thr_btn_e",
                short_label: "E",
                full_name: "Botón E (Pulgar)",
                norm_x: 0.43,
                norm_y: 0.15,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_btn_i",
                short_label: "I",
                full_name: "Botón I",
                norm_x: 0.57,
                norm_y: 0.15,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_ministick_push",
                short_label: "MINI",
                full_name: "Minipalanca Push (Pulgar)",
                norm_x: 0.69,
                norm_y: 0.15,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_btn_h",
                short_label: "H",
                full_name: "Botón H",
                norm_x: 0.50,
                norm_y: 0.22,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_k1_up",
                short_label: "K1▲",
                full_name: "Interruptor K1 Arriba",
                norm_x: 0.78,
                norm_y: 0.22,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_k1_down",
                short_label: "K1▼",
                full_name: "Interruptor K1 Abajo",
                norm_x: 0.78,
                norm_y: 0.28,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección C: HAT 3
            SchematicNode {
                id: "thr_h3_up",
                short_label: "3▲",
                full_name: "HAT 3 Arriba",
                norm_x: 0.43,
                norm_y: 0.30,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_right",
                short_label: "3▶",
                full_name: "HAT 3 Derecha",
                norm_x: 0.49,
                norm_y: 0.33,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_down",
                short_label: "3▼",
                full_name: "HAT 3 Abajo",
                norm_x: 0.43,
                norm_y: 0.36,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_left",
                short_label: "3◀",
                full_name: "HAT 3 Izquierda",
                norm_x: 0.37,
                norm_y: 0.33,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección C: HAT 4
            SchematicNode {
                id: "thr_h4_up",
                short_label: "4▲",
                full_name: "HAT 4 Arriba",
                norm_x: 0.65,
                norm_y: 0.30,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_right",
                short_label: "4▶",
                full_name: "HAT 4 Derecha",
                norm_x: 0.71,
                norm_y: 0.33,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_down",
                short_label: "4▼",
                full_name: "HAT 4 Abajo",
                norm_x: 0.65,
                norm_y: 0.36,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_left",
                short_label: "4◀",
                full_name: "HAT 4 Izquierda",
                norm_x: 0.59,
                norm_y: 0.33,
                radius: 9.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección G: Conmutadores de Base TGL 1 a 7 (SW 1..14)
            SchematicNode {
                id: "thr_sw_1",
                short_label: "SW1",
                full_name: "TGL 1 Arriba (SW 1)",
                norm_x: 0.35,
                norm_y: 0.60,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_2",
                short_label: "SW2",
                full_name: "TGL 1 Abajo (SW 2)",
                norm_x: 0.35,
                norm_y: 0.66,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_3",
                short_label: "SW3",
                full_name: "TGL 2 Arriba (SW 3)",
                norm_x: 0.50,
                norm_y: 0.60,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_4",
                short_label: "SW4",
                full_name: "TGL 2 Abajo (SW 4)",
                norm_x: 0.50,
                norm_y: 0.66,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_5",
                short_label: "SW5",
                full_name: "TGL 3 Arriba (SW 5)",
                norm_x: 0.65,
                norm_y: 0.60,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_6",
                short_label: "SW6",
                full_name: "TGL 3 Abajo (SW 6)",
                norm_x: 0.65,
                norm_y: 0.66,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_7",
                short_label: "SW7",
                full_name: "TGL 4 Arriba (SW 7)",
                norm_x: 0.80,
                norm_y: 0.60,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_8",
                short_label: "SW8",
                full_name: "TGL 4 Abajo (SW 8)",
                norm_x: 0.80,
                norm_y: 0.66,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_9",
                short_label: "SW9",
                full_name: "TGL 5 Arriba (SW 9)",
                norm_x: 0.35,
                norm_y: 0.78,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_10",
                short_label: "SW10",
                full_name: "TGL 5 Abajo (SW 10)",
                norm_x: 0.35,
                norm_y: 0.84,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_11",
                short_label: "SW11",
                full_name: "TGL 6 Arriba (SW 11)",
                norm_x: 0.50,
                norm_y: 0.78,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_12",
                short_label: "SW12",
                full_name: "TGL 6 Abajo (SW 12)",
                norm_x: 0.50,
                norm_y: 0.84,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_13",
                short_label: "SW13",
                full_name: "TGL 7 Arriba (SW 13)",
                norm_x: 0.65,
                norm_y: 0.78,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
            SchematicNode {
                id: "thr_sw_14",
                short_label: "SW14",
                full_name: "TGL 7 Abajo (SW 14)",
                norm_x: 0.65,
                norm_y: 0.84,
                radius: 9.0,
                is_rect: true,
                rect_size: Vec2::new(26.0, 15.0),
            },
        ],
    }
}
