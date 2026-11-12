//! Integración con la bandeja del sistema (System Tray / StatusNotifierItem) para x56linux.
//!
//! Permite que x56-gui continúe ejecutándose en segundo plano (minimizada en el tray)
//! al cerrar la ventana, manteniendo el daemon uinput y el mapeo activo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use ksni::blocking::TrayMethods;
use ksni::{menu::*, Category, Icon, MenuItem, Status, ToolTip, Tray};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    HideWindow,
    ToggleWindow,
    Quit,
}

pub struct X56Tray {
    tx: Sender<TrayCommand>,
    egui_ctx: egui::Context,
    active_profile: Arc<Mutex<String>>,
    uinput_online: Arc<Mutex<bool>>,
    window_visible: Arc<AtomicBool>,
    pixmaps: Vec<Icon>,
}

impl Tray for X56Tray {
    fn id(&self) -> String {
        "x56linux".into()
    }

    fn title(&self) -> String {
        "X-56 RHINO Control Center".into()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_name(&self) -> String {
        "x56linux".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.pixmaps.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        let prof = self
            .active_profile
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| "Default".into());
        let uinput = if self.uinput_online.lock().map(|g| *g).unwrap_or(true) {
            "Online (1000Hz)"
        } else {
            "Offline"
        };
        ToolTip {
            title: "X-56 RHINO // RUST-EVDEV".into(),
            description: format!("Perfil: {prof} | uinput: {uinput}"),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCommand::ToggleWindow);
        self.egui_ctx.request_repaint();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let prof = self
            .active_profile
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| "Default".into());
        let uinput_str = if self.uinput_online.lock().map(|g| *g).unwrap_or(true) {
            "✓ Uinput Daemon: Online"
        } else {
            "✗ Uinput Daemon: Offline"
        };
        let is_vis = self.window_visible.load(Ordering::Relaxed);
        let toggle_label = if is_vis {
            "Ocultar ventana"
        } else {
            "Mostrar ventana"
        };

        vec![
            StandardItem {
                label: toggle_label.into(),
                icon_name: "input-gaming".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(TrayCommand::ToggleWindow);
                    this.egui_ctx.request_repaint();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: format!("Perfil activo: {prof}"),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: uinput_str.into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Cerrar por completo".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(TrayCommand::Quit);
                    this.egui_ctx.request_repaint();
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub struct TrayHandle {
    pub rx: Receiver<TrayCommand>,
    pub active_profile: Arc<Mutex<String>>,
    pub uinput_online: Arc<Mutex<bool>>,
    pub window_visible: Arc<AtomicBool>,
    _handle: Option<ksni::blocking::Handle<X56Tray>>,
    _x11_shutdown: Option<Sender<()>>,
}

fn spawn_x11_xembed_tray(tx: Sender<TrayCommand>, egui_ctx: egui::Context) -> Option<Sender<()>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt as _;
    use x11rb::protocol::xproto::*;
    use x11rb::protocol::Event;
    use x11rb::wrapper::ConnectionExt as _;

    std::env::var_os("DISPLAY")?;

    let (shutdown_tx, shutdown_rx) = channel::<()>();

    std::thread::Builder::new()
        .name("x11-tray-dock".into())
        .spawn(move || {
            let (conn, screen_num) = match x11rb::connect(None) {
                Ok(c) => c,
                Err(err) => {
                    log::debug!("X11 no disponible para XEmbed tray: {err}");
                    return;
                }
            };

            let atom_tray = match conn.intern_atom(false, b"_NET_SYSTEM_TRAY_S0") {
                Ok(c) => match c.reply() {
                    Ok(r) => r.atom,
                    Err(_) => return,
                },
                Err(_) => return,
            };

            let owner = match conn.get_selection_owner(atom_tray) {
                Ok(c) => match c.reply() {
                    Ok(r) => r.owner,
                    Err(_) => return,
                },
                Err(_) => return,
            };

            if owner == x11rb::NONE {
                log::debug!("No se detectó dock XEmbed (_NET_SYSTEM_TRAY_S0 sin propietario)");
                return;
            }

            let atom_opcode = match conn.intern_atom(false, b"_NET_SYSTEM_TRAY_OPCODE") {
                Ok(c) => match c.reply() {
                    Ok(r) => r.atom,
                    Err(_) => return,
                },
                Err(_) => return,
            };

            let atom_xembed_info = match conn.intern_atom(false, b"_XEMBED_INFO") {
                Ok(c) => match c.reply() {
                    Ok(r) => r.atom,
                    Err(_) => return,
                },
                Err(_) => return,
            };

            let screen = &conn.setup().roots[screen_num];
            let win_id = match conn.generate_id() {
                Ok(id) => id,
                Err(_) => return,
            };

            let mut cur_w = 21u16;
            let mut cur_h = 21u16;
            let values = CreateWindowAux::new()
                .background_pixmap(BackPixmap::PARENT_RELATIVE)
                .event_mask(
                    EventMask::EXPOSURE | EventMask::BUTTON_PRESS | EventMask::STRUCTURE_NOTIFY,
                );

            if conn
                .create_window(
                    screen.root_depth,
                    win_id,
                    screen.root,
                    0,
                    0,
                    cur_w,
                    cur_h,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    screen.root_visual,
                    &values,
                )
                .is_err()
            {
                return;
            }

            let _ = conn.change_property32(
                PropMode::REPLACE,
                win_id,
                atom_xembed_info,
                atom_xembed_info,
                &[0, 1],
            );

            let _ = conn.change_property8(
                PropMode::REPLACE,
                win_id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"x56linux",
            );
            let _ = conn.change_property8(
                PropMode::REPLACE,
                win_id,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"x56linux\0x56linux\0",
            );

            let dock_event = ClientMessageEvent {
                response_type: CLIENT_MESSAGE_EVENT,
                format: 32,
                sequence: 0,
                window: owner,
                type_: atom_opcode,
                data: ClientMessageData::from([0, 0, win_id, 0, 0]),
            };

            let _ = conn.send_event(false, owner, EventMask::NO_EVENT, dock_event);
            let _ = conn.flush();

            let icon_bytes = include_bytes!("../assets/tray_icon_22.png");
            let img = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png)
                .map(|i| i.to_rgba8())
                .ok();

            let gc = match conn.generate_id() {
                Ok(id) => {
                    let _ = conn.create_gc(id, win_id, &CreateGCAux::new());
                    id
                }
                Err(_) => 0,
            };

            log::info!("Dock XEmbed para Polybar/i3 conectado exitosamente (win: {win_id:#x})");

            let render_icon = |w: u16, h: u16| -> Option<Vec<u8>> {
                let rgba = img.as_ref()?;
                let scaled = if rgba.width() as u16 != w || rgba.height() as u16 != h {
                    image::imageops::resize(
                        rgba,
                        w as u32,
                        h as u32,
                        image::imageops::FilterType::Lanczos3,
                    )
                } else {
                    rgba.clone()
                };

                // Intentar leer el fondo del contenedor Polybar para mezcla alfa perfecta
                let bg_raw = conn
                    .get_image(ImageFormat::Z_PIXMAP, win_id, 0, 0, w, h, !0)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .map(|r| r.data);

                let len = (w as usize) * (h as usize) * 4;
                let mut out = vec![0u8; len];

                for (idx, chunk) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let x = (idx % (w as usize)) as u32;
                    let y = (idx / (w as usize)) as u32;
                    let p = scaled.get_pixel(x, y);

                    let alpha = p[3] as f32 / 255.0;
                    let inv = 1.0 - alpha;

                    let (bg_b, bg_g, bg_r) = if let Some(ref bg) = bg_raw {
                        let base = idx * 4;
                        if base + 2 < bg.len() {
                            (bg[base] as f32, bg[base + 1] as f32, bg[base + 2] as f32)
                        } else {
                            (30.0, 30.0, 30.0)
                        }
                    } else {
                        (30.0, 30.0, 30.0)
                    };

                    chunk[0] = ((p[2] as f32 * alpha) + (bg_b * inv)).clamp(0.0, 255.0) as u8;
                    chunk[1] = ((p[1] as f32 * alpha) + (bg_g * inv)).clamp(0.0, 255.0) as u8;
                    chunk[2] = ((p[0] as f32 * alpha) + (bg_r * inv)).clamp(0.0, 255.0) as u8;
                    chunk[3] = 0;
                }

                Some(out)
            };

            let menu_n_data = png_to_bgra(include_bytes!("../assets/menu_normal.png"));
            let menu_h0_data = png_to_bgra(include_bytes!("../assets/menu_hover_0.png"));
            let menu_h1_data = png_to_bgra(include_bytes!("../assets/menu_hover_1.png"));

            let show_context_menu = |root_x: i16, root_y: i16| {
                let (menu_w, menu_h, bgra_n) = match menu_n_data {
                    Some(ref d) => (d.0, d.1, &d.2),
                    None => return,
                };
                let bgra_0 = menu_h0_data.as_ref().map(|d| &d.2).unwrap_or(bgra_n);
                let bgra_1 = menu_h1_data.as_ref().map(|d| &d.2).unwrap_or(bgra_n);
                let item_h = menu_h / 2;

                let max_x = screen.width_in_pixels.saturating_sub(menu_w) as i16;
                let max_y = screen.height_in_pixels.saturating_sub(menu_h) as i16;
                let menu_x = root_x.clamp(4, max_x.max(4));
                let menu_y = if root_y + (menu_h as i16) > max_y {
                    (root_y - (menu_h as i16)).max(4)
                } else {
                    root_y.clamp(4, max_y.max(4))
                };

                let menu_id = match conn.generate_id() {
                    Ok(id) => id,
                    Err(_) => return,
                };

                let menu_vals = CreateWindowAux::new()
                    .override_redirect(1)
                    .background_pixel(0x11141b)
                    .border_pixel(0x00f0ff)
                    .event_mask(
                        EventMask::EXPOSURE
                            | EventMask::BUTTON_PRESS
                            | EventMask::POINTER_MOTION
                            | EventMask::LEAVE_WINDOW,
                    );

                if conn
                    .create_window(
                        screen.root_depth,
                        menu_id,
                        screen.root,
                        menu_x,
                        menu_y,
                        menu_w,
                        menu_h,
                        1,
                        WindowClass::INPUT_OUTPUT,
                        screen.root_visual,
                        &menu_vals,
                    )
                    .is_err()
                {
                    return;
                }

                let menu_gc = match conn.generate_id() {
                    Ok(id) => {
                        let _ = conn.create_gc(id, menu_id, &CreateGCAux::new());
                        id
                    }
                    Err(_) => 0,
                };

                let _ = conn.map_window(menu_id);
                let _ = conn.flush();

                let _ = conn.grab_pointer(
                    false,
                    menu_id,
                    EventMask::BUTTON_PRESS | EventMask::POINTER_MOTION | EventMask::BUTTON_RELEASE,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                );
                let _ = conn.flush();

                let draw_menu_state = |hover: Option<usize>| {
                    if menu_gc == 0 {
                        return;
                    }
                    let data = match hover {
                        Some(0) => bgra_0,
                        Some(1) => bgra_1,
                        _ => bgra_n,
                    };
                    let _ = conn.put_image(
                        ImageFormat::Z_PIXMAP,
                        menu_id,
                        menu_gc,
                        menu_w,
                        menu_h,
                        0,
                        0,
                        0,
                        screen.root_depth,
                        data,
                    );
                    let _ = conn.flush();
                };

                draw_menu_state(None);

                let mut current_hover: Option<usize> = None;
                let mut active = true;

                while active {
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    match conn.poll_for_event() {
                        Ok(Some(Event::MotionNotify(mev))) => {
                            let new_hover = if mev.event_x >= 0
                                && mev.event_x < (menu_w as i16)
                                && mev.event_y >= 0
                                && mev.event_y < (menu_h as i16)
                            {
                                Some((mev.event_y as u16 / item_h) as usize)
                            } else {
                                None
                            };
                            if new_hover != current_hover {
                                current_hover = new_hover;
                                draw_menu_state(current_hover);
                            }
                        }
                        Ok(Some(Event::Expose(eev))) => {
                            if eev.window == menu_id {
                                draw_menu_state(current_hover);
                            }
                        }
                        Ok(Some(Event::ButtonPress(bev))) => {
                            if bev.event == menu_id
                                && bev.event_x >= 0
                                && bev.event_x < (menu_w as i16)
                                && bev.event_y >= 0
                                && bev.event_y < (menu_h as i16)
                            {
                                let clicked = (bev.event_y as u16 / item_h) as usize;
                                if clicked == 0 {
                                    let _ = tx.send(TrayCommand::ToggleWindow);
                                    egui_ctx.request_repaint();
                                } else {
                                    let _ = tx.send(TrayCommand::Quit);
                                    egui_ctx.request_repaint();
                                }
                            }
                            active = false;
                        }
                        Ok(Some(Event::LeaveNotify(lev))) => {
                            if lev.event == menu_id {
                                current_hover = None;
                                draw_menu_state(None);
                            }
                        }
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                        Err(_) => break,
                    }
                }

                let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
                if menu_gc != 0 {
                    let _ = conn.free_gc(menu_gc);
                }
                let _ = conn.destroy_window(menu_id);
                let _ = conn.flush();
            };

            loop {
                if shutdown_rx.try_recv().is_ok() {
                    let _ = conn.destroy_window(win_id);
                    let _ = conn.flush();
                    break;
                }

                match conn.poll_for_event() {
                    Ok(Some(Event::ConfigureNotify(ev))) => {
                        cur_w = ev.width;
                        cur_h = ev.height;
                    }
                    Ok(Some(Event::Expose(_))) => {
                        let _ = conn.clear_area(false, win_id, 0, 0, cur_w, cur_h);
                        let _ = conn.flush();

                        if let Some(data) = render_icon(cur_w, cur_h) {
                            if gc != 0 {
                                let _ = conn.put_image(
                                    ImageFormat::Z_PIXMAP,
                                    win_id,
                                    gc,
                                    cur_w,
                                    cur_h,
                                    0,
                                    0,
                                    0,
                                    screen.root_depth,
                                    &data,
                                );
                                let _ = conn.flush();
                            }
                        }
                    }
                    Ok(Some(Event::ButtonPress(ev))) => {
                        if ev.detail == 1 {
                            let _ = tx.send(TrayCommand::ToggleWindow);
                            egui_ctx.request_repaint();
                        } else if ev.detail == 3 {
                            show_context_menu(ev.root_x, ev.root_y);
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        })
        .ok()?;

    Some(shutdown_tx)
}

fn png_to_bgra(bytes: &[u8]) -> Option<(u16, u16, Vec<u8>)> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let w = rgba.width() as u16;
    let h = rgba.height() as u16;
    let mut bgra = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for &[r, g, b, _a] in rgba.as_chunks::<4>().0 {
        bgra.push(b);
        bgra.push(g);
        bgra.push(r);
        bgra.push(0);
    }
    Some((w, h, bgra))
}

fn load_png_as_icon(png_bytes: &[u8]) -> Option<Icon> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let width = rgba.width() as i32;
    let height = rgba.height() as i32;
    let raw = rgba.into_raw();
    let mut data = Vec::with_capacity(raw.len());
    for &[r, g, b, a] in raw.as_chunks::<4>().0 {
        data.push(a);
        data.push(r);
        data.push(g);
        data.push(b);
    }
    Some(Icon {
        width,
        height,
        data,
    })
}

pub fn load_window_icon() -> Option<egui::IconData> {
    let png_bytes = include_bytes!("../assets/tray_icon_64.png");
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

pub fn start_tray(egui_ctx: egui::Context, initial_profile: &str) -> TrayHandle {
    let (tx, rx) = channel();
    let active_profile = Arc::new(Mutex::new(initial_profile.to_string()));
    let uinput_online = Arc::new(Mutex::new(true));
    let window_visible = Arc::new(AtomicBool::new(true));

    let mut pixmaps = Vec::new();
    if let Some(i32) = load_png_as_icon(include_bytes!("../assets/tray_icon_32.png")) {
        pixmaps.push(i32);
    }
    if let Some(i64) = load_png_as_icon(include_bytes!("../assets/tray_icon_64.png")) {
        pixmaps.push(i64);
    }

    let x11_shutdown = spawn_x11_xembed_tray(tx.clone(), egui_ctx.clone());

    let tray = X56Tray {
        tx,
        egui_ctx,
        active_profile: active_profile.clone(),
        uinput_online: uinput_online.clone(),
        window_visible: window_visible.clone(),
        pixmaps,
    };

    let handle = match tray.assume_sni_available(true).spawn() {
        Ok(h) => {
            log::info!("Bandeja del sistema (StatusNotifierItem tray) inicializada con éxito");
            Some(h)
        }
        Err(err) => {
            log::warn!("No se pudo inicializar la bandeja del sistema (tray): {err}");
            None
        }
    };

    TrayHandle {
        rx,
        active_profile,
        uinput_online,
        window_visible,
        _handle: handle,
        _x11_shutdown: x11_shutdown,
    }
}
