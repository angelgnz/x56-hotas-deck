//! Pestaña de Programación (Programming) del Logitech G X56 HOTAS (Sección 8 del Manual).
//!
//! Permite la configuración de celdas de comandos por botón en los modos `M1`, `M2` y `S1`,
//! visualización en esquema técnico interactivo 2D, inyección de pulsaciones de teclado
//! vía uinput y ventana de pruebas en tiempo real.

use eframe::egui::{self, Color32, Key, RichText, Stroke, Ui, Vec2};
use x56_core::{
    catalog_for_device, ButtonDescriptor, ButtonMonitor, CommandCell, DeviceKind,
    LiveButtonsSnapshot, ModeSlot, ProgrammingProfile, TriggeredCommandEvent,
};

/// Estado y lógica de la pestaña de Programación (Sección 8).
pub struct ProgrammingTab {
    /// Dispositivo seleccionado actualmente para edición (Stick o Throttle).
    pub selected_device: DeviceKind,
    /// Modo activo seleccionado en el editor (`M1`, `M2`, `S1`).
    pub selected_mode: ModeSlot,
    /// Si es `true`, sincroniza automáticamente `selected_mode` con el selector físico del Throttle.
    pub follow_hardware_mode: bool,
    /// Alterna entre visualización dividida (Esquema 2D + Celdas) o pantalla completa (Solo Celdas).
    pub show_diagram_view: bool,
    /// Perfil de programación con las asignaciones de celdas por modo.
    pub profile: ProgrammingProfile,
    /// ID del botón cuya celda está actualmente en modo edición.
    pub editing_button_id: Option<String>,
    /// Teclas capturadas mientras la celda está en edición antes de confirmar o cancelar.
    pub pending_keys: Vec<String>,
    /// Nombre descriptivo del comando en edición (p. ej. "Tren de aterrizaje").
    pub pending_name: String,
    /// ID del botón a iluminar en el diagrama 2D (por hover de mouse o pulsación física).
    pub hovered_button_id: Option<String>,
    /// Controla la visibilidad de la ventana modal "Probar Perfil".
    pub show_test_window: bool,
    /// Historial de comandos disparados registrados durante la prueba.
    pub test_log: Vec<TriggeredCommandEvent>,
    /// Filtro opcional por grupo/sección funcional del manual ("A", "B", "C", etc.).
    pub filter_group: Option<String>,
    /// Mensaje de estado informativo o de error: `(texto, es_error)`.
    pub status_message: Option<(String, bool)>,
    /// Campo de texto de prueba para verificar pulsaciones reales de teclado.
    pub test_input_text: String,
}

impl Default for ProgrammingTab {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgrammingTab {
    /// Crea una nueva instancia de la pestaña de programación con valores iniciales.
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
            "Perfil de programación cargado correctamente.".to_string(),
            false,
        ));
    }

    /// Devuelve una copia del perfil de programación actual.
    pub fn current_programming_profile(&self) -> ProgrammingProfile {
        self.profile.clone()
    }

    /// Renderiza la interfaz de usuario de la pestaña de programación.
    pub fn ui(&mut self, ui: &mut Ui, monitor: &ButtonMonitor) {
        // 1. Obtener snapshot del hardware y sincronizar selector físico
        let snapshot = monitor.snapshot();
        if self.follow_hardware_mode {
            self.selected_mode = snapshot.active_mode;
        }

        // Si hay botones físicos presionados y no hay hover de mouse activo, iluminar el primero
        if self.hovered_button_id.is_none() {
            if let Some(first_pressed) = snapshot.pressed_buttons.iter().next() {
                self.hovered_button_id = Some(first_pressed.clone());
            }
        }

        // 2. Captura de teclas en vivo cuando una celda está en edición
        self.handle_live_key_capture(ui);

        // 3. Barra de herramientas superior del Editor de Perfiles (Sección 8)
        self.render_top_toolbar(ui, monitor);

        ui.add_space(4.0);

        // Mensaje de estado temporal si existe
        let mut dismiss_status = false;
        if let Some((ref msg, is_err)) = self.status_message {
            let col = if is_err {
                Color32::from_rgb(255, 110, 110)
            } else {
                Color32::from_rgb(100, 230, 140)
            };
            ui.horizontal(|ui| {
                ui.colored_label(col, msg);
                if ui.small_button("✖").clicked() {
                    dismiss_status = true;
                }
            });
            ui.add_space(2.0);
        }
        if dismiss_status {
            self.status_message = None;
        }

        // 4. Barra del Selector Rotativo de Modo (M1 | M2 | S1) y Deslizante SLD
        self.render_mode_bar(ui, &snapshot);

        ui.add_space(6.0);

        // 5. Área principal: Esquema 2D interactivo (columna izq) + Celdas de comandos (columna der)
        if self.show_diagram_view {
            ui.horizontal_top(|ui| {
                // Columna izquierda: Diagrama interactivo del dispositivo
                ui.allocate_ui_with_layout(
                    Vec2::new(410.0, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.render_diagram(ui, &snapshot);
                    },
                );

                ui.separator();

                // Columna derecha: Lista de Celdas de Comandos
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.render_command_cells(ui, monitor, &snapshot);
                    });
            });
        } else {
            // Modo Solo Celdas (pantalla completa)
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.render_command_cells(ui, monitor, &snapshot);
                });
        }

        // 6. Ventana Modal "Probar Perfil"
        if self.show_test_window {
            self.render_test_modal(ui, monitor, &snapshot);
        }
    }

    /// Captura pulsaciones de teclado en vivo para la celda en edición.
    fn handle_live_key_capture(&mut self, ui: &mut Ui) {
        if self.editing_button_id.is_none() {
            return;
        }

        // Si el usuario está escribiendo en el campo de texto del nombre, no capturar como atajos
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
                    // Si pulsa Backspace y hay teclas capturadas, elimina la última
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

    /// Barra de herramientas superior fiel al manual de usuario X56 (Sección 8).
    fn render_top_toolbar(&mut self, ui: &mut Ui, monitor: &ButtonMonitor) {
        egui::Frame::canvas(ui.style())
            .fill(Color32::from_rgb(20, 26, 36))
            .stroke(Stroke::new(1.0, Color32::from_rgb(45, 65, 90)))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Botón Nuevo (Limpiar perfil)
                    if ui
                        .button(RichText::new("📄 Nuevo").strong())
                        .on_hover_text("Limpiar todas las asignaciones del perfil actual")
                        .clicked()
                    {
                        self.profile = ProgrammingProfile::default();
                        monitor.set_profile(self.profile.clone());
                        self.editing_button_id = None;
                        self.pending_keys.clear();
                        self.pending_name.clear();
                        self.status_message = Some((
                            "Todas las asignaciones han sido reiniciadas.".to_string(),
                            false,
                        ));
                    }

                    ui.separator();

                    // Botón Vistas (Alternar Diagrama + Celdas vs Solo Celdas)
                    let views_label = if self.show_diagram_view {
                        "👁️ Vistas: Esquema + Celdas"
                    } else {
                        "👁️ Vistas: Solo Celdas"
                    };
                    if ui
                        .selectable_label(self.show_diagram_view, views_label)
                        .on_hover_text("Alternar visualización de esquema 2D del hardware")
                        .clicked()
                    {
                        self.show_diagram_view = !self.show_diagram_view;
                    }

                    ui.separator();

                    // Botón Diana Azul (Activar Perfil)
                    let active = monitor.is_active();
                    let uinput_ok = monitor.uinput_available();

                    let (target_text, target_color) = if active {
                        if uinput_ok {
                            ("🎯 Perfil Activo (Emisión ON)", Color32::from_rgb(0, 220, 255))
                        } else {
                            (
                                "⚠️ Perfil Activo (Sin /dev/uinput)",
                                Color32::from_rgb(255, 175, 40),
                            )
                        }
                    } else {
                        (
                            "🎯 Activar Perfil (Diana Azul)",
                            Color32::from_rgb(160, 180, 200),
                        )
                    };

                    if ui
                        .button(
                            RichText::new(target_text)
                                .color(target_color)
                                .strong(),
                        )
                        .on_hover_text(
                            "Activa o pausa la emisión en vivo de pulsaciones de teclado vía /dev/uinput al pulsar los botones físicos",
                        )
                        .clicked()
                    {
                        let new_state = !active;
                        monitor.set_active(new_state);
                        if new_state {
                            if uinput_ok {
                                self.status_message = Some((
                                    "🎯 Perfil activado: Las teclas configuradas se emitirán al sistema al pulsar botones físicos.".to_string(),
                                    false,
                                ));
                            } else {
                                self.status_message = Some((
                                    "Aviso: /dev/uinput no accesible. Instala las reglas udev o añade permisos para emisión de teclas.".to_string(),
                                    true,
                                ));
                            }
                        } else {
                            self.status_message = Some((
                                "Emisión de teclas en pausa (Diana Azul desactivada).".to_string(),
                                false,
                            ));
                        }
                    }

                    ui.separator();

                    // Botón Probar Perfil (Engranaje)
                    if ui
                        .button(RichText::new("⚙️ Probar Perfil").strong())
                        .on_hover_text(
                            "Abre la ventana de prueba para monitorizar eventos y pulsaciones en tiempo real",
                        )
                        .clicked()
                    {
                        self.show_test_window = true;
                    }

                    ui.separator();

                    // Selector de Dispositivo (Stick / Throttle)
                    ui.label(
                        RichText::new("Dispositivo:")
                            .color(Color32::from_rgb(170, 190, 210))
                            .size(12.0),
                    );

                    let stick_sel = self.selected_device == DeviceKind::Stick;
                    if ui
                        .selectable_label(stick_sel, "🕹️ Palanca (Stick)")
                        .clicked()
                        && !stick_sel
                    {
                        self.selected_device = DeviceKind::Stick;
                        self.editing_button_id = None;
                        self.hovered_button_id = None;
                    }

                    let thr_sel = self.selected_device == DeviceKind::Throttle;
                    if ui
                        .selectable_label(thr_sel, "🚀 Acelerador (Throttle)")
                        .clicked()
                        && !thr_sel
                    {
                        self.selected_device = DeviceKind::Throttle;
                        self.editing_button_id = None;
                        self.hovered_button_id = None;
                    }
                });
            });
    }

    /// Barra de estado del selector físico M1/M2/S1, SLD y selección de modo en el editor.
    fn render_mode_bar(&mut self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        egui::Frame::canvas(ui.style())
            .fill(Color32::from_rgb(16, 22, 32))
            .stroke(Stroke::new(1.0, Color32::from_rgb(40, 58, 82)))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Estado físico en vivo (Hardware)
                    ui.label(
                        RichText::new("HARDWARE:")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(140, 165, 190)),
                    );

                    // Badge Modo Físico
                    let phys_col = match snapshot.active_mode {
                        ModeSlot::M1 => Color32::from_rgb(0, 220, 255),
                        ModeSlot::M2 => Color32::from_rgb(255, 160, 40),
                        ModeSlot::S1 => Color32::from_rgb(210, 90, 255),
                    };
                    render_badge(
                        ui,
                        &format!("MODO FÍSICO: {}", snapshot.active_mode.as_str()),
                        phys_col,
                    );

                    // Badge Control Deslizante B (SLD)
                    let (sld_text, sld_col) = if snapshot.slider_sld_on {
                        ("SLD: ACTIVO (ON)", Color32::from_rgb(50, 255, 120))
                    } else {
                        ("SLD: INACTIVO (OFF)", Color32::from_rgb(130, 140, 150))
                    };
                    render_badge(ui, sld_text, sld_col);

                    ui.separator();

                    // Modos en el Editor
                    ui.label(
                        RichText::new("MODO EDITOR:")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(140, 165, 190)),
                    );

                    for &mode in &ModeSlot::ALL {
                        let count = self
                            .profile
                            .bindings_for(mode)
                            .cells
                            .values()
                            .filter(|c| c.is_assigned())
                            .count();
                        let is_active_mode = self.selected_mode == mode;

                        let text = format!("{} ({})", mode.description(), count);
                        let rich_text = if is_active_mode {
                            RichText::new(&text)
                                .color(Color32::WHITE)
                                .strong()
                        } else {
                            RichText::new(&text).color(Color32::from_rgb(175, 195, 215))
                        };

                        if ui.selectable_label(is_active_mode, rich_text).clicked() {
                            self.selected_mode = mode;
                            self.editing_button_id = None;
                        }
                    }

                    ui.separator();

                    // Checkbox para seguir el selector físico
                    ui.checkbox(
                        &mut self.follow_hardware_mode,
                        RichText::new("🔄 Seguir selector físico M1/M2/S1").size(12.0),
                    )
                    .on_hover_text(
                        "Cambia automáticamente entre M1, M2 y S1 cuando giras el selector físico del Throttle",
                    );
                });
            });
    }

    /// Dibuja el diagrama esquemático técnico 2D interactivo con `egui::Painter`.
    fn render_diagram(&mut self, ui: &mut Ui, snapshot: &LiveButtonsSnapshot) {
        let desired_size = Vec2::new(ui.available_width(), 650.0);
        let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::click());
        let rect = response.rect;

        // Fondo técnico estilo blueprint
        painter.rect_filled(rect, 8.0, Color32::from_rgb(14, 20, 30));
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.5, Color32::from_rgb(38, 58, 86)),
            egui::StrokeKind::Inside,
        );

        // Rejilla técnica de fondo
        let grid_step = 32.0;
        let mut x = rect.min.x + grid_step;
        while x < rect.max.x {
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                Stroke::new(0.5, Color32::from_rgba_unmultiplied(45, 70, 100, 35)),
            );
            x += grid_step;
        }
        let mut y = rect.min.y + grid_step;
        while y < rect.max.y {
            painter.line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                Stroke::new(0.5, Color32::from_rgba_unmultiplied(45, 70, 100, 35)),
            );
            y += grid_step;
        }

        // Título del diagrama
        let header_title = match self.selected_device {
            DeviceKind::Stick => "STICK X56 — ESQUEMA TÉCNICO INTERACTIVO",
            DeviceKind::Throttle => "THROTTLE X56 — ESQUEMA TÉCNICO INTERACTIVO",
        };
        painter.text(
            egui::pos2(rect.min.x + 12.0, rect.min.y + 16.0),
            egui::Align2::LEFT_CENTER,
            header_title,
            egui::FontId::proportional(13.0),
            Color32::from_rgb(0, 220, 255),
        );

        // Dibujar siluetas esquemáticas de fondo según el dispositivo
        match self.selected_device {
            DeviceKind::Stick => {
                self.draw_stick_silhouette(&painter, rect);
            }
            DeviceKind::Throttle => {
                self.draw_throttle_silhouette(&painter, rect, snapshot);
            }
        }

        // Obtener nodos del dispositivo actual
        let nodes = get_device_schematic_nodes(self.selected_device);

        let mouse_pos = ui.input(|i| i.pointer.hover_pos());
        let clicked = response.clicked();

        let mut newly_hovered: Option<String> = None;
        let mut clicked_button_id: Option<String> = None;

        for node in &nodes {
            let center = egui::pos2(
                rect.min.x + node.norm_x * rect.width(),
                rect.min.y + node.norm_y * rect.height(),
            );

            // Detección de puntero sobre el nodo
            let is_mouse_over = if let Some(mpos) = mouse_pos {
                if node.is_rect {
                    let half = node.rect_size * 0.5;
                    mpos.x >= center.x - half.x
                        && mpos.x <= center.x + half.x
                        && mpos.y >= center.y - half.y
                        && mpos.y <= center.y + half.y
                } else {
                    (mpos - center).length() <= node.radius + 3.0
                }
            } else {
                false
            };

            if is_mouse_over {
                newly_hovered = Some(node.id.to_string());
                if clicked {
                    clicked_button_id = Some(node.id.to_string());
                }
            }

            // Estados del botón
            let is_physically_pressed = snapshot.pressed_buttons.contains(node.id);
            let is_hovered = self.hovered_button_id.as_deref() == Some(node.id) || is_mouse_over;
            let is_assigned = self
                .profile
                .get_cell(self.selected_mode, node.id)
                .is_some_and(|c| c.is_assigned());
            let is_editing = self.editing_button_id.as_deref() == Some(node.id);

            // Determinación de colores (Verde Neón -> Cian -> Ámbar -> Reposo)
            let (fill, stroke, text_col) = if is_physically_pressed {
                (
                    Color32::from_rgb(0, 255, 120),
                    Stroke::new(2.5, Color32::from_rgb(220, 255, 230)),
                    Color32::BLACK,
                )
            } else if is_editing {
                (
                    Color32::from_rgb(255, 215, 0),
                    Stroke::new(2.5, Color32::WHITE),
                    Color32::BLACK,
                )
            } else if is_hovered {
                (
                    Color32::from_rgb(0, 215, 255),
                    Stroke::new(2.0, Color32::WHITE),
                    Color32::BLACK,
                )
            } else if is_assigned {
                (
                    Color32::from_rgb(255, 185, 30),
                    Stroke::new(1.5, Color32::from_rgb(255, 235, 160)),
                    Color32::BLACK,
                )
            } else {
                (
                    Color32::from_rgb(28, 42, 60),
                    Stroke::new(1.0, Color32::from_rgb(65, 95, 130)),
                    Color32::from_rgb(180, 205, 230),
                )
            };

            // Glow exterior para botones presionados o activos
            if is_physically_pressed {
                painter.circle_stroke(
                    center,
                    node.radius + 4.0,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(0, 255, 120, 130)),
                );
            } else if is_hovered {
                painter.circle_stroke(
                    center,
                    node.radius + 3.0,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(0, 215, 255, 110)),
                );
            }

            // Dibujar nodo
            if node.is_rect {
                let node_rect = egui::Rect::from_center_size(center, node.rect_size);
                painter.rect_filled(node_rect, 3.0, fill);
                painter.rect_stroke(node_rect, 3.0, stroke, egui::StrokeKind::Inside);
            } else {
                painter.circle_filled(center, node.radius, fill);
                painter.circle_stroke(center, node.radius, stroke);
            }

            // Etiqueta corta del botón
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                node.short_label,
                egui::FontId::monospace(10.0),
                text_col,
            );

            // Tooltip interactivo si el ratón está encima
            if is_mouse_over {
                let cell_info = if let Some(c) = self.profile.get_cell(self.selected_mode, node.id)
                {
                    if c.is_assigned() {
                        format!("Comando: {} [{}]", c.name, c.keys_display())
                    } else {
                        "Sin comando asignado".to_string()
                    }
                } else {
                    "Sin comando asignado".to_string()
                };

                response.clone().on_hover_ui_at_pointer(|ui| {
                    ui.heading(node.full_name);
                    ui.label(
                        RichText::new(format!("ID: {}", node.id))
                            .size(10.0)
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(cell_info)
                            .strong()
                            .color(Color32::from_rgb(0, 220, 255)),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new("Haz clic en el nodo para editar asignación")
                            .size(11.0)
                            .italics(),
                    );
                });
            }
        }

        if let Some(hovered) = newly_hovered {
            self.hovered_button_id = Some(hovered);
        }

        if let Some(btn_id) = clicked_button_id {
            self.editing_button_id = Some(btn_id.clone());
            if let Some(cell) = self.profile.get_cell(self.selected_mode, &btn_id) {
                self.pending_name = cell.name.clone();
                self.pending_keys = cell.keys.clone();
            } else {
                self.pending_name = btn_id.clone();
                self.pending_keys.clear();
            }
        }

        // Leyenda visual al pie del esquema
        self.render_diagram_legend(&painter, rect);
    }

    /// Dibuja los elementos vectoriales del Stick en el esquema 2D.
    fn draw_stick_silhouette(&self, painter: &egui::Painter, rect: egui::Rect) {
        let stroke_subtle = Stroke::new(1.0, Color32::from_rgb(40, 65, 95));
        let stroke_accent = Stroke::new(1.2, Color32::from_rgb(55, 95, 140));

        // Cabezal superior del Stick
        let head_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.16,
                rect.min.y + rect.height() * 0.08,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.84,
                rect.min.y + rect.height() * 0.44,
            ),
        );
        painter.rect_stroke(head_rect, 10.0, stroke_accent, egui::StrokeKind::Inside);

        // Separadores de zonas funcionales del cabezal
        painter.line_segment(
            [
                egui::pos2(
                    rect.min.x + rect.width() * 0.16,
                    rect.min.y + rect.height() * 0.22,
                ),
                egui::pos2(
                    rect.min.x + rect.width() * 0.84,
                    rect.min.y + rect.height() * 0.22,
                ),
            ],
            stroke_subtle,
        );
        painter.line_segment(
            [
                egui::pos2(
                    rect.min.x + rect.width() * 0.50,
                    rect.min.y + rect.height() * 0.08,
                ),
                egui::pos2(
                    rect.min.x + rect.width() * 0.50,
                    rect.min.y + rect.height() * 0.35,
                ),
            ],
            stroke_subtle,
        );

        // Mástil central (Shaft)
        let shaft_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.42,
                rect.min.y + rect.height() * 0.44,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.58,
                rect.min.y + rect.height() * 0.72,
            ),
        );
        painter.rect_filled(shaft_rect, 4.0, Color32::from_rgb(18, 25, 38));
        painter.rect_stroke(shaft_rect, 4.0, stroke_accent, egui::StrokeKind::Inside);

        // Anillo de muelle / Gimbal
        let center_ring = egui::pos2(
            rect.min.x + rect.width() * 0.50,
            rect.min.y + rect.height() * 0.76,
        );
        painter.circle_stroke(center_ring, rect.width() * 0.14, stroke_subtle);
        painter.circle_stroke(center_ring, rect.width() * 0.08, stroke_accent);

        // Base rectangular del Stick
        let base_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.14,
                rect.min.y + rect.height() * 0.80,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.86,
                rect.min.y + rect.height() * 0.92,
            ),
        );
        painter.rect_filled(base_rect, 6.0, Color32::from_rgb(16, 23, 34));
        painter.rect_stroke(base_rect, 6.0, stroke_accent, egui::StrokeKind::Inside);

        // Textos de secciones del manual
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.18,
                rect.min.y + rect.height() * 0.10,
            ),
            egui::Align2::LEFT_TOP,
            "SEC. A (POV)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.82,
                rect.min.y + rect.height() * 0.10,
            ),
            egui::Align2::RIGHT_TOP,
            "SEC. B (MINIPALANCA)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.50,
                rect.min.y + rect.height() * 0.23,
            ),
            egui::Align2::CENTER_TOP,
            "SEC. C (HATS THUMB H1 & H2)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.50,
                rect.min.y + rect.height() * 0.35,
            ),
            egui::Align2::CENTER_TOP,
            "SEC. D (GATILLO & CABEZAL)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.50,
                rect.min.y + rect.height() * 0.52,
            ),
            egui::Align2::CENTER_TOP,
            "SEC. F (PINKIE & FLYING PINKIE)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
    }

    /// Dibuja los elementos vectoriales del Throttle en el esquema 2D.
    fn draw_throttle_silhouette(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let stroke_subtle = Stroke::new(1.0, Color32::from_rgb(40, 65, 95));
        let stroke_accent = Stroke::new(1.2, Color32::from_rgb(55, 95, 140));

        // Empuñaduras dobles gemelas (Twin Levers)
        let lever_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.15,
                rect.min.y + rect.height() * 0.08,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.85,
                rect.min.y + rect.height() * 0.44,
            ),
        );
        painter.rect_filled(lever_rect, 8.0, Color32::from_rgb(16, 23, 34));
        painter.rect_stroke(lever_rect, 8.0, stroke_accent, egui::StrokeKind::Inside);

        // Separador central de acelerador dividido
        painter.line_segment(
            [
                egui::pos2(
                    rect.min.x + rect.width() * 0.50,
                    rect.min.y + rect.height() * 0.08,
                ),
                egui::pos2(
                    rect.min.x + rect.width() * 0.50,
                    rect.min.y + rect.height() * 0.44,
                ),
            ],
            stroke_subtle,
        );

        // Base consola del Throttle
        let base_rect = egui::Rect::from_min_max(
            egui::pos2(
                rect.min.x + rect.width() * 0.10,
                rect.min.y + rect.height() * 0.48,
            ),
            egui::pos2(
                rect.min.x + rect.width() * 0.90,
                rect.min.y + rect.height() * 0.92,
            ),
        );
        painter.rect_filled(base_rect, 8.0, Color32::from_rgb(14, 21, 31));
        painter.rect_stroke(base_rect, 8.0, stroke_accent, egui::StrokeKind::Inside);

        // Indicador del selector rotativo F de modo físico en la base
        let knob_center = egui::pos2(
            rect.min.x + rect.width() * 0.20,
            rect.min.y + rect.height() * 0.58,
        );
        painter.circle_filled(knob_center, 18.0, Color32::from_rgb(24, 34, 48));
        painter.circle_stroke(knob_center, 18.0, stroke_accent);

        let mode_angle: f32 = match snapshot.active_mode {
            ModeSlot::M1 => -0.6,
            ModeSlot::M2 => 0.0,
            ModeSlot::S1 => 0.6,
        };
        let ind_end = egui::pos2(
            knob_center.x + 14.0 * mode_angle.sin(),
            knob_center.y - 14.0 * mode_angle.cos(),
        );
        painter.line_segment(
            [knob_center, ind_end],
            Stroke::new(2.5, Color32::from_rgb(0, 220, 255)),
        );

        painter.text(
            knob_center,
            egui::Align2::CENTER_CENTER,
            snapshot.active_mode.as_str(),
            egui::FontId::monospace(10.0),
            Color32::WHITE,
        );

        // Rótulos de secciones del manual
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.16,
                rect.min.y + rect.height() * 0.09,
            ),
            egui::Align2::LEFT_TOP,
            "SEC. A (GIRATORIOS F/G & SCROLL)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.84,
                rect.min.y + rect.height() * 0.09,
            ),
            egui::Align2::RIGHT_TOP,
            "SEC. B (SLD)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.50,
                rect.min.y + rect.height() * 0.26,
            ),
            egui::Align2::CENTER_TOP,
            "SEC. C (PULGAR & HATS H3/H4)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.20,
                rect.min.y + rect.height() * 0.50,
            ),
            egui::Align2::CENTER_TOP,
            "SEC. F (MODO)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
        painter.text(
            egui::pos2(
                rect.min.x + rect.width() * 0.58,
                rect.min.y + rect.height() * 0.52,
            ),
            egui::Align2::CENTER_TOP,
            "SEC. G: CONMUTADORES DE BASE (TGL 1-7)",
            egui::FontId::monospace(9.0),
            Color32::from_rgb(100, 140, 180),
        );
    }

    /// Renderiza la leyenda de estados de color al pie del esquema 2D.
    fn render_diagram_legend(&self, painter: &egui::Painter, rect: egui::Rect) {
        let ly = rect.max.y - 18.0;
        let mut lx = rect.min.x + 16.0;

        // Verde Neón
        painter.circle_filled(egui::pos2(lx, ly), 5.0, Color32::from_rgb(0, 255, 120));
        painter.text(
            egui::pos2(lx + 8.0, ly),
            egui::Align2::LEFT_CENTER,
            "Pulsado Físico",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(180, 200, 220),
        );
        lx += 105.0;

        // Cian Brillante
        painter.circle_filled(egui::pos2(lx, ly), 5.0, Color32::from_rgb(0, 215, 255));
        painter.text(
            egui::pos2(lx + 8.0, ly),
            egui::Align2::LEFT_CENTER,
            "Señalado / Hover",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(180, 200, 220),
        );
        lx += 115.0;

        // Ámbar
        painter.circle_filled(egui::pos2(lx, ly), 5.0, Color32::from_rgb(255, 185, 30));
        painter.text(
            egui::pos2(lx + 8.0, ly),
            egui::Align2::LEFT_CENTER,
            "Asignado en Modo",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(180, 200, 220),
        );
    }

    /// Columna derecha: Lista estructurada de Celdas de Comandos agrupadas por sección.
    fn render_command_cells(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        let catalog = catalog_for_device(self.selected_device);

        // Agrupación respetando el orden del catálogo
        let mut groups: Vec<(&str, Vec<&ButtonDescriptor>)> = Vec::new();
        for btn in &catalog {
            if let Some((_, list)) = groups.iter_mut().find(|(g, _)| *g == btn.group) {
                list.push(btn);
            } else {
                groups.push((btn.group, vec![btn]));
            }
        }

        // Barra de filtrado superior
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Filtrar por sección:")
                    .color(Color32::from_rgb(160, 180, 200))
                    .size(12.0),
            );

            let current_filter = self
                .filter_group
                .as_deref()
                .unwrap_or("Todas las secciones");

            egui::ComboBox::from_id_salt("cmd_group_filter")
                .selected_text(current_filter)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.filter_group.is_none(), "Todas las secciones")
                        .clicked()
                    {
                        self.filter_group = None;
                    }
                    for (group_name, _) in &groups {
                        let is_sel = self.filter_group.as_deref() == Some(*group_name);
                        if ui.selectable_label(is_sel, *group_name).clicked() {
                            self.filter_group = Some((*group_name).to_string());
                        }
                    }
                });

            if self.filter_group.is_some() && ui.small_button("✖ Limpiar filtro").clicked() {
                self.filter_group = None;
            }
        });

        ui.add_space(6.0);

        // Renderizado de grupos
        for (group_name, buttons) in groups {
            if let Some(ref filter) = self.filter_group {
                if filter != group_name {
                    continue;
                }
            }

            let assigned_in_group = buttons
                .iter()
                .filter(|b| {
                    self.profile
                        .get_cell(self.selected_mode, &b.id)
                        .is_some_and(|c| c.is_assigned())
                })
                .count();

            // Encabezado de grupo
            egui::Frame::canvas(ui.style())
                .fill(Color32::from_rgb(22, 30, 42))
                .stroke(Stroke::new(1.0, Color32::from_rgb(50, 75, 105)))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(
                            RichText::new(group_name)
                                .size(13.0)
                                .color(Color32::from_rgb(0, 220, 255)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Asignados: {} / {}",
                                    assigned_in_group,
                                    buttons.len()
                                ))
                                .size(11.0)
                                .color(Color32::from_rgb(160, 180, 200)),
                            );
                        });
                    });
                });

            ui.add_space(3.0);

            // Celdas dentro del grupo
            for btn in buttons {
                self.render_single_command_cell(ui, monitor, snapshot, btn);
                ui.add_space(4.0);
            }

            ui.add_space(6.0);
        }
    }

    /// Renderiza una celda individual de comando para un botón específico.
    fn render_single_command_cell(
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
        let has_assignment = cell_opt.as_ref().is_some_and(|c| c.is_assigned());

        // Colores del marco de la celda
        let (border_stroke, fill_color) = if is_editing {
            (
                Stroke::new(2.0, Color32::from_rgb(255, 215, 0)),
                Color32::from_rgb(26, 32, 44),
            )
        } else if is_physically_pressed {
            (
                Stroke::new(2.0, Color32::from_rgb(0, 255, 120)),
                Color32::from_rgb(20, 36, 32),
            )
        } else if is_hovered {
            (
                Stroke::new(1.5, Color32::from_rgb(0, 215, 255)),
                Color32::from_rgb(22, 32, 46),
            )
        } else if has_assignment {
            (
                Stroke::new(1.0, Color32::from_rgb(80, 110, 145)),
                Color32::from_rgb(18, 24, 34),
            )
        } else {
            (
                Stroke::new(1.0, Color32::from_rgb(38, 52, 70)),
                Color32::from_rgb(16, 21, 30),
            )
        };

        let frame_resp = egui::Frame::canvas(ui.style())
            .fill(fill_color)
            .stroke(border_stroke)
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 7))
            .show(ui, |ui| {
                if is_editing {
                    // Modo Edición activo
                    self.render_editing_cell_contents(ui, monitor, btn);
                } else {
                    // Modo Visualización normal
                    self.render_view_cell_contents(
                        ui,
                        monitor,
                        btn,
                        is_physically_pressed,
                        cell_opt.as_ref(),
                    );
                }
            });

        // Hover sobre la celda ilumina el botón correspondiente en el esquema 2D
        if frame_resp.response.hovered() {
            self.hovered_button_id = Some(btn.id.clone());
        }
    }

    /// Renderiza el contenido de una celda en modo visualización.
    fn render_view_cell_contents(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        btn: &ButtonDescriptor,
        is_physically_pressed: bool,
        cell_opt: Option<&CommandCell>,
    ) {
        ui.horizontal(|ui| {
            // Etiqueta del botón: [Código Manual] Nombre
            let code_badge = format!("[{}]", btn.code_label);
            ui.colored_label(
                Color32::from_rgb(0, 220, 255),
                RichText::new(code_badge).monospace().strong(),
            );
            ui.label(RichText::new(btn.label).strong().color(Color32::WHITE));

            // Indicador físico ● PULSADO
            if is_physically_pressed {
                render_badge(ui, "● PULSADO", Color32::from_rgb(0, 255, 120));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Botón papelera para borrar asignación
                if cell_opt.is_some_and(|c| c.is_assigned())
                    && ui
                        .button(RichText::new("🗑").color(Color32::from_rgb(255, 120, 120)))
                        .on_hover_text("Eliminar asignación de este botón en el modo actual")
                        .clicked()
                {
                    self.profile.remove_cell(self.selected_mode, &btn.id);
                    monitor.set_profile(self.profile.clone());
                    self.status_message =
                        Some((format!("Asignación eliminada para {}", btn.label), false));
                }

                // Botón / Mosaico para editar
                let edit_clicked = if let Some(cell) = cell_opt {
                    if cell.is_assigned() {
                        let resp = ui
                            .horizontal(|ui| {
                                ui.label(
                                    RichText::new(&cell.name)
                                        .strong()
                                        .color(Color32::from_rgb(255, 230, 160)),
                                );
                                render_key_tiles(ui, &cell.keys, false);
                            })
                            .response;
                        resp.clicked()
                    } else {
                        ui.button(
                            RichText::new("Haga clic para asignar comando...")
                                .italics()
                                .color(Color32::from_rgb(140, 160, 180)),
                        )
                        .clicked()
                    }
                } else {
                    ui.button(
                        RichText::new("Haga clic para asignar comando...")
                            .italics()
                            .color(Color32::from_rgb(140, 160, 180)),
                    )
                    .clicked()
                };

                if edit_clicked {
                    self.editing_button_id = Some(btn.id.clone());
                    if let Some(cell) = cell_opt {
                        self.pending_name = cell.name.clone();
                        self.pending_keys = cell.keys.clone();
                    } else {
                        self.pending_name = btn.label.to_string();
                        self.pending_keys.clear();
                    }
                }
            });
        });
    }

    /// Renderiza el contenido de una celda en modo edición activa.
    fn render_editing_cell_contents(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        btn: &ButtonDescriptor,
    ) {
        ui.vertical(|ui| {
            // Encabezado con cursor parpadeante
            let time = ui.input(|i| i.time);
            let cursor = if (time * 2.0).fract() < 0.5 {
                "▌"
            } else {
                " "
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("[{}] {}", btn.code_label, btn.label))
                        .strong()
                        .color(Color32::from_rgb(0, 220, 255)),
                );
                ui.colored_label(
                    Color32::from_rgb(255, 215, 0),
                    format!("{cursor} Esperando pulsación de teclado..."),
                );
            });

            ui.add_space(4.0);

            // Mosaicos visuales de teclas capturadas
            ui.horizontal(|ui| {
                ui.label(RichText::new("Teclas asignadas:").size(12.0));
                if self.pending_keys.is_empty() {
                    ui.label(
                        RichText::new("(Presiona teclas en tu teclado físico para capturarlas)")
                            .italics()
                            .color(Color32::from_rgb(160, 180, 200)),
                    );
                } else {
                    render_key_tiles(ui, &self.pending_keys, true);
                    if ui.small_button("Limpiar teclas").clicked() {
                        self.pending_keys.clear();
                    }
                }
            });

            ui.add_space(4.0);

            // Campo para el nombre del comando y botones de confirmación / cancelación
            ui.horizontal(|ui| {
                ui.label(RichText::new("Nombre del comando:").size(12.0));
                ui.add(
                    egui::TextEdit::singleline(&mut self.pending_name)
                        .desired_width(180.0)
                        .hint_text("Ej. Tren de aterrizaje"),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Botón Cancelar ✖
                    if ui
                        .button(
                            RichText::new("✖ Cancelar")
                                .color(Color32::from_rgb(255, 100, 100))
                                .strong(),
                        )
                        .clicked()
                    {
                        self.editing_button_id = None;
                        self.pending_keys.clear();
                        self.pending_name.clear();
                    }

                    // Botón Confirmar ✔
                    if ui
                        .button(
                            RichText::new("✔ Confirmar")
                                .color(Color32::from_rgb(50, 255, 120))
                                .strong(),
                        )
                        .clicked()
                    {
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
                            Some((format!("Asignación guardada para {}", btn.label), false));

                        self.editing_button_id = None;
                        self.pending_keys.clear();
                        self.pending_name.clear();
                    }
                });
            });
        });
    }

    /// Renderiza la ventana modal "⚙️ Probar Perfil (Test)".
    fn render_test_modal(
        &mut self,
        ui: &mut Ui,
        monitor: &ButtonMonitor,
        snapshot: &LiveButtonsSnapshot,
    ) {
        // Extraer eventos recién emitidos por el monitor
        let new_events = monitor.drain_events();
        self.test_log.extend(new_events);
        if self.test_log.len() > 300 {
            self.test_log.drain(0..self.test_log.len() - 300);
        }

        let mut close_modal = false;
        let mut open = self.show_test_window;

        egui::Window::new("⚙️ Probar Perfil (Test de Comandos)")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(720.0, 500.0))
            .show(ui.ctx(), |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Monitor de Comandos Disparados en Vivo");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑 Limpiar Registro").clicked() {
                                self.test_log.clear();
                            }
                        });
                    });

                    ui.label(
                        RichText::new(
                            "Presiona cualquier botón físico en el X56 para verificar la detección y la emisión de comandos.",
                        )
                        .size(12.0)
                        .color(Color32::from_rgb(170, 190, 210)),
                    );

                    ui.add_space(4.0);

                    // Indicadores de estado de inyección
                    ui.horizontal(|ui| {
                        ui.label("Modo activo:");
                        render_badge(
                            ui,
                            snapshot.active_mode.as_str(),
                            Color32::from_rgb(0, 220, 255),
                        );

                        ui.separator();

                        ui.label("Inyección uinput:");
                        if monitor.is_active() {
                            render_badge(
                                ui,
                                "EMISIÓN ACTIVA (DIANA ON)",
                                Color32::from_rgb(50, 255, 120),
                            );
                        } else {
                            render_badge(
                                ui,
                                "EN PAUSA (DIANA OFF)",
                                Color32::from_rgb(255, 175, 40),
                            );
                        }

                        ui.separator();

                        if monitor.uinput_available() {
                            render_badge(
                                ui,
                                "/dev/uinput OK",
                                Color32::from_rgb(50, 255, 120),
                            );
                        } else {
                            render_badge(
                                ui,
                                "/dev/uinput NO DISPONIBLE",
                                Color32::from_rgb(255, 80, 80),
                            );
                        }
                    });

                    ui.add_space(6.0);

                    // Campo de prueba de pulsaciones
                    ui.label(
                        RichText::new("Campo de prueba (haz clic dentro para verificar la escritura de teclas):")
                            .size(12.0)
                            .strong(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.test_input_text)
                            .hint_text("Haz clic aquí y pulsa un botón físico con la Diana activada..."),
                    );

                    ui.add_space(6.0);
                    ui.separator();

                    // Tabla de eventos registrados
                    ui.label(
                        RichText::new(format!(
                            "Historial de eventos ({} registrados):",
                            self.test_log.len()
                        ))
                        .strong(),
                    );

                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.test_log.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(20.0);
                                    ui.label(
                                        RichText::new("Esperando pulsación de botones físicos...")
                                            .italics()
                                            .color(Color32::GRAY),
                                    );
                                    ui.add_space(20.0);
                                });
                            } else {
                                egui::Grid::new("test_events_grid")
                                    .striped(true)
                                    .spacing(Vec2::new(12.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.strong("Hora");
                                        ui.strong("Modo");
                                        ui.strong("Botón Físico");
                                        ui.strong("Comando");
                                        ui.strong("Teclas Emitidas");
                                        ui.strong("Estado");
                                        ui.end_row();

                                        for ev in &self.test_log {
                                            ui.monospace(&ev.timestamp_hms);
                                            render_badge(
                                                ui,
                                                ev.mode.as_str(),
                                                Color32::from_rgb(0, 200, 240),
                                            );
                                            ui.label(&ev.button_label);
                                            ui.strong(&ev.command_name);
                                            render_key_tiles(ui, &ev.keys, false);
                                            if ev.uinput_emitted {
                                                ui.colored_label(
                                                    Color32::from_rgb(50, 255, 120),
                                                    "✔ Emitido",
                                                );
                                            } else {
                                                ui.colored_label(
                                                    Color32::from_rgb(220, 220, 100),
                                                    "ℹ Simulado",
                                                );
                                            }
                                            ui.end_row();
                                        }
                                    });
                            }
                        });

                    ui.add_space(6.0);
                    ui.separator();

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Aceptar / Cerrar").strong()).clicked() {
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

// ---------------------------------------------------------------------------
// Helpers de Renderizado y Mapeo
// ---------------------------------------------------------------------------

/// Renderiza un badge visual estilo píldora con color de acento y borde.
fn render_badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::canvas(ui.style())
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            35,
        ))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(11.0).strong());
        });
}

/// Renderiza mosaicos visuales (tiles blancos con texto oscuro) para cada tecla asignada.
fn render_key_tiles(ui: &mut Ui, keys: &[String], large: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (i, key) in keys.iter().enumerate() {
            if i > 0 {
                ui.colored_label(Color32::from_rgb(140, 160, 180), "+");
            }
            let (margin_x, margin_y, font_size) = if large { (8, 4, 13.0) } else { (5, 2, 11.0) };

            egui::Frame::canvas(ui.style())
                .fill(Color32::from_rgb(240, 244, 248)) // Mosaico blanco / gris muy claro
                .stroke(Stroke::new(1.0, Color32::from_rgb(180, 195, 210)))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(margin_x, margin_y))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(key)
                            .color(Color32::from_rgb(20, 25, 35)) // Texto oscuro de alto contraste
                            .strong()
                            .size(font_size),
                    );
                });
        }
    });
}

/// Mapea una tecla de egui a un nombre canónico compatible con `x56_core`.
fn map_key_to_name(key: Key) -> Option<&'static str> {
    match key {
        Key::ControlLeft => Some("LCtrl"),
        Key::ControlRight => Some("RCtrl"),
        Key::ShiftLeft => Some("LShift"),
        Key::ShiftRight => Some("RShift"),
        Key::AltLeft => Some("LAlt"),
        Key::AltRight => Some("RAlt"),
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

// ---------------------------------------------------------------------------
// Definición de Nodos Esquemáticos para el Diagrama 2D
// ---------------------------------------------------------------------------

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
            // Sección A: POV 8-Way
            SchematicNode {
                id: "stick_pov_up",
                short_label: "P▲",
                full_name: "POV Vista Arriba",
                norm_x: 0.35,
                norm_y: 0.12,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_right",
                short_label: "P▶",
                full_name: "POV Vista Derecha",
                norm_x: 0.42,
                norm_y: 0.15,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_down",
                short_label: "P▼",
                full_name: "POV Vista Abajo",
                norm_x: 0.35,
                norm_y: 0.18,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_pov_left",
                short_label: "P◀",
                full_name: "POV Vista Izquierda",
                norm_x: 0.28,
                norm_y: 0.15,
                radius: 11.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección B: Minipalanca Push (Botón C)
            SchematicNode {
                id: "stick_btn_c",
                short_label: "C",
                full_name: "Botón C (Minipalanca Push)",
                norm_x: 0.72,
                norm_y: 0.15,
                radius: 13.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección C: HAT 1 (Castle)
            SchematicNode {
                id: "stick_h1_up",
                short_label: "1▲",
                full_name: "HAT 1 (Castle) Arriba",
                norm_x: 0.28,
                norm_y: 0.26,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_right",
                short_label: "1▶",
                full_name: "HAT 1 (Castle) Derecha",
                norm_x: 0.35,
                norm_y: 0.29,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_down",
                short_label: "1▼",
                full_name: "HAT 1 (Castle) Abajo",
                norm_x: 0.28,
                norm_y: 0.32,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h1_left",
                short_label: "1◀",
                full_name: "HAT 1 (Castle) Izquierda",
                norm_x: 0.21,
                norm_y: 0.29,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección C: HAT 2 (Witches)
            SchematicNode {
                id: "stick_h2_up",
                short_label: "2▲",
                full_name: "HAT 2 (Witches) Arriba",
                norm_x: 0.72,
                norm_y: 0.26,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_right",
                short_label: "2▶",
                full_name: "HAT 2 (Witches) Derecha",
                norm_x: 0.79,
                norm_y: 0.29,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_down",
                short_label: "2▼",
                full_name: "HAT 2 (Witches) Abajo",
                norm_x: 0.72,
                norm_y: 0.32,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_h2_left",
                short_label: "2◀",
                full_name: "HAT 2 (Witches) Izquierda",
                norm_x: 0.65,
                norm_y: 0.29,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección D: Gatillo y Botones A/B/D
            SchematicNode {
                id: "stick_btn_a",
                short_label: "A",
                full_name: "Botón A (Cabezal Superior)",
                norm_x: 0.50,
                norm_y: 0.15,
                radius: 13.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_trigger",
                short_label: "TRG",
                full_name: "Gatillo Principal (Trigger)",
                norm_x: 0.50,
                norm_y: 0.39,
                radius: 14.0,
                is_rect: true,
                rect_size: Vec2::new(36.0, 20.0),
            },
            SchematicNode {
                id: "stick_btn_b",
                short_label: "B",
                full_name: "Botón B (Lateral Pulgar)",
                norm_x: 0.78,
                norm_y: 0.39,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "stick_btn_d",
                short_label: "D",
                full_name: "Botón D (Pulgar Inferior)",
                norm_x: 0.22,
                norm_y: 0.39,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            // Sección F: Pinkie y Flying Pinkie
            SchematicNode {
                id: "stick_pinkie",
                short_label: "PNK",
                full_name: "Botón Meñique (Pinkie)",
                norm_x: 0.44,
                norm_y: 0.59,
                radius: 12.0,
                is_rect: true,
                rect_size: Vec2::new(32.0, 18.0),
            },
            SchematicNode {
                id: "stick_flying_pinkie",
                short_label: "FLY",
                full_name: "Palanca Meñique (Flying Pinkie)",
                norm_x: 0.56,
                norm_y: 0.68,
                radius: 12.0,
                is_rect: true,
                rect_size: Vec2::new(36.0, 18.0),
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
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_rty_g_push",
                short_label: "RTY G",
                full_name: "Giratorio G Push (RTY 2)",
                norm_x: 0.22,
                norm_y: 0.21,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_scroll_fwd",
                short_label: "SC▲",
                full_name: "Rueda Scroll Adelante",
                norm_x: 0.22,
                norm_y: 0.28,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_scroll_bck",
                short_label: "SC▼",
                full_name: "Rueda Scroll Atrás",
                norm_x: 0.22,
                norm_y: 0.35,
                radius: 10.0,
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
                radius: 12.0,
                is_rect: true,
                rect_size: Vec2::new(34.0, 18.0),
            },
            // Sección C: Botones Pulgar E, H, I, Minipalanca y K1
            SchematicNode {
                id: "thr_btn_e",
                short_label: "E",
                full_name: "Botón E (Pulgar)",
                norm_x: 0.43,
                norm_y: 0.15,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_btn_i",
                short_label: "I",
                full_name: "Botón I",
                norm_x: 0.57,
                norm_y: 0.15,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_ministick_push",
                short_label: "MINI",
                full_name: "Minipalanca Push (Pulgar)",
                norm_x: 0.69,
                norm_y: 0.15,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_btn_h",
                short_label: "H",
                full_name: "Botón H",
                norm_x: 0.50,
                norm_y: 0.22,
                radius: 12.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_k1_up",
                short_label: "K1▲",
                full_name: "Interruptor K1 Arriba",
                norm_x: 0.78,
                norm_y: 0.22,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_k1_down",
                short_label: "K1▼",
                full_name: "Interruptor K1 Abajo",
                norm_x: 0.78,
                norm_y: 0.28,
                radius: 10.0,
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
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_right",
                short_label: "3▶",
                full_name: "HAT 3 Derecha",
                norm_x: 0.49,
                norm_y: 0.33,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_down",
                short_label: "3▼",
                full_name: "HAT 3 Abajo",
                norm_x: 0.43,
                norm_y: 0.36,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h3_left",
                short_label: "3◀",
                full_name: "HAT 3 Izquierda",
                norm_x: 0.37,
                norm_y: 0.33,
                radius: 10.0,
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
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_right",
                short_label: "4▶",
                full_name: "HAT 4 Derecha",
                norm_x: 0.71,
                norm_y: 0.33,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_down",
                short_label: "4▼",
                full_name: "HAT 4 Abajo",
                norm_x: 0.65,
                norm_y: 0.36,
                radius: 10.0,
                is_rect: false,
                rect_size: Vec2::ZERO,
            },
            SchematicNode {
                id: "thr_h4_left",
                short_label: "4◀",
                full_name: "HAT 4 Izquierda",
                norm_x: 0.59,
                norm_y: 0.33,
                radius: 10.0,
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
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_2",
                short_label: "SW2",
                full_name: "TGL 1 Abajo (SW 2)",
                norm_x: 0.35,
                norm_y: 0.66,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_3",
                short_label: "SW3",
                full_name: "TGL 2 Arriba (SW 3)",
                norm_x: 0.50,
                norm_y: 0.60,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_4",
                short_label: "SW4",
                full_name: "TGL 2 Abajo (SW 4)",
                norm_x: 0.50,
                norm_y: 0.66,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_5",
                short_label: "SW5",
                full_name: "TGL 3 Arriba (SW 5)",
                norm_x: 0.65,
                norm_y: 0.60,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_6",
                short_label: "SW6",
                full_name: "TGL 3 Abajo (SW 6)",
                norm_x: 0.65,
                norm_y: 0.66,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_7",
                short_label: "SW7",
                full_name: "TGL 4 Arriba (SW 7)",
                norm_x: 0.80,
                norm_y: 0.60,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_8",
                short_label: "SW8",
                full_name: "TGL 4 Abajo (SW 8)",
                norm_x: 0.80,
                norm_y: 0.66,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_9",
                short_label: "SW9",
                full_name: "TGL 5 Arriba (SW 9)",
                norm_x: 0.35,
                norm_y: 0.78,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_10",
                short_label: "SW10",
                full_name: "TGL 5 Abajo (SW 10)",
                norm_x: 0.35,
                norm_y: 0.84,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_11",
                short_label: "SW11",
                full_name: "TGL 6 Arriba (SW 11)",
                norm_x: 0.50,
                norm_y: 0.78,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_12",
                short_label: "SW12",
                full_name: "TGL 6 Abajo (SW 12)",
                norm_x: 0.50,
                norm_y: 0.84,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_13",
                short_label: "SW13",
                full_name: "TGL 7 Arriba (SW 13)",
                norm_x: 0.65,
                norm_y: 0.78,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
            SchematicNode {
                id: "thr_sw_14",
                short_label: "SW14",
                full_name: "TGL 7 Abajo (SW 14)",
                norm_x: 0.65,
                norm_y: 0.84,
                radius: 10.0,
                is_rect: true,
                rect_size: Vec2::new(28.0, 16.0),
            },
        ],
    }
}
