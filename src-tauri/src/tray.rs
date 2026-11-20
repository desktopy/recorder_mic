//! Eco en el área de notificación: ícono con estado, menú, flyout (`widget`), píldora flotante,
//! cierre a bandeja, inicio con Windows y salida ordenada.

use crate::commands::AppState;
use crate::flyout::{self, Rect};
use crate::icons::{self, IconState};
use crate::prefs::{self, AppPrefs, PrefsStore};
use eco_audio::types::{EngineErrorPayload, Status};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent, Wry};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt;

pub const TRAY_ID: &str = "eco-tray";
pub const WIDGET: &str = "widget";
pub const PILL: &str = "pill";
const WIDGET_SIZE: (f64, f64) = (320.0, 220.0);
const PILL_SIZE: (f64, f64) = (216.0, 52.0);
/// Margen entre el flyout y la barra / bordes (lógico).
const FLYOUT_GAP: f64 = 12.0;
/// Tras perder el foco el flyout se oculta; un clic en el ícono dentro de esta ventana lo trata como «cerrar».
const BLUR_GUARD: Duration = Duration::from_millis(450);
const DBLCLICK_GUARD: Duration = Duration::from_millis(500);
const HINT_MS: u64 = 6000;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/* ───────────────────────── estado de la bandeja ───────────────────────── */

struct TrayParts {
    tray: TrayIcon<Wry>,
    toggle: MenuItem<Wry>,
    pause: MenuItem<Wry>,
    auto: CheckMenuItem<Wry>,
    /// Lo último aplicado, para tocar la API solo cuando algo cambia.
    look: Option<(IconState, bool, u32)>,
    tip: String,
    toggle_text: String,
    pause_text: String,
    pause_enabled: bool,
    toggle_enabled: bool,
    auto_checked: bool,
    light: bool,
}

pub struct Shell {
    pub prefs: PrefsStore,
    parts: Mutex<Option<TrayParts>>,
    tray_ok: AtomicBool,
    quitting: AtomicBool,
    last_icon: Mutex<Option<Rect>>,
    widget_blur_at: Mutex<Option<Instant>>,
    last_dbl: Mutex<Option<Instant>>,
    widget_gen: AtomicU64,
    pill_gen: AtomicU64,
    pill_shown: AtomicBool,
    pill_ignore_move_until: Mutex<Instant>,
}

impl Shell {
    pub fn new() -> Self {
        Self {
            prefs: PrefsStore::load(),
            parts: Mutex::new(None),
            tray_ok: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            last_icon: Mutex::new(None),
            widget_blur_at: Mutex::new(None),
            last_dbl: Mutex::new(None),
            widget_gen: AtomicU64::new(0),
            pill_gen: AtomicU64::new(0),
            pill_shown: AtomicBool::new(false),
            pill_ignore_move_until: Mutex::new(Instant::now()),
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

/* ───────────────────────── Win32 mínimo ───────────────────────── */

mod sys {
    #[link(name = "user32")]
    extern "system" {
        pub fn GetSystemMetrics(index: i32) -> i32;
        pub fn GetWindowRect(hwnd: isize, r: *mut Rc) -> i32;
        pub fn GetClientRect(hwnd: isize, r: *mut Rc) -> i32;
        pub fn ClientToScreen(hwnd: isize, p: *mut Pt) -> i32;
        pub fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    #[link(name = "advapi32")]
    extern "system" {
        pub fn RegGetValueW(hkey: isize, sub: *const u16, value: *const u16, flags: u32, ty: *mut u32, data: *mut core::ffi::c_void, len: *mut u32) -> i32;
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct Rc {
        pub l: i32,
        pub t: i32,
        pub r: i32,
        pub b: i32,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct Pt {
        pub x: i32,
        pub y: i32,
    }
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SM_CXSMICON: i32 = 49;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Tamaño del ícono pequeño del sistema (16 a 100 %, 20 a 125 %, 24 a 150 %, 32 a 200 %).
fn tray_icon_size() -> u32 {
    let s = unsafe { sys::GetSystemMetrics(sys::SM_CXSMICON) };
    if (12..=64).contains(&s) {
        s as u32
    } else {
        16
    }
}

/// `true` si la barra de tareas usa el tema claro de Windows.
fn taskbar_is_light() -> bool {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let val = wide("SystemUsesLightTheme");
    let mut data: u32 = 0;
    let mut len: u32 = 4;
    // HKEY_CURRENT_USER = 0x80000001; RRF_RT_REG_DWORD = 0x10
    let r = unsafe { sys::RegGetValueW(0x8000_0001u32 as i32 as isize, sub.as_ptr(), val.as_ptr(), 0x10, std::ptr::null_mut(), (&mut data as *mut u32).cast(), &mut len) };
    r == 0 && data != 0
}

/* ───────────────────────── textos ───────────────────────── */

pub fn fmt_clock(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, sec) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

pub fn tooltip_for(status: Status, elapsed_ms: u64) -> String {
    match status {
        Status::Idle => "Eco — Listo".into(),
        Status::Recording => format!("Eco — Grabando {}", fmt_clock(elapsed_ms)),
        Status::Paused => format!("Eco — En pausa {}", fmt_clock(elapsed_ms)),
        Status::Finalizing => "Eco — Guardando…".into(),
    }
}

fn icon_state_for(status: Status) -> IconState {
    match status {
        Status::Recording => IconState::Recording,
        Status::Paused => IconState::Paused,
        _ => IconState::Idle,
    }
}

fn toggle_text(status: Status) -> &'static str {
    match status {
        Status::Idle => "Iniciar grabación",
        Status::Recording | Status::Paused => "Detener grabación",
        Status::Finalizing => "Guardando…",
    }
}

fn pause_text(status: Status) -> &'static str {
    if status == Status::Paused {
        "Reanudar"
    } else {
        "Pausar"
    }
}

/* ───────────────────────── arranque ───────────────────────── */

pub fn launched_hidden() -> bool {
    std::env::args().any(|a| a == "--hidden")
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let shell = app.state::<Shell>();

    // El valor real del inicio con Windows lo manda el registro.
    if let Ok(on) = app.autolaunch().is_enabled() {
        let _ = shell.prefs.update(|p| p.launch_at_startup = on);
    }
    let prefs = shell.prefs.get();

    let toggle = MenuItem::with_id(app, "toggle", toggle_text(Status::Idle), true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", pause_text(Status::Idle), false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Abrir Eco", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Abrir carpeta de grabaciones", true, None::<&str>)?;
    let auto = CheckMenuItem::with_id(app, "autostart", "Iniciar con Windows", true, prefs.launch_at_startup, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle, &pause, &sep1, &open, &folder, &sep2, &auto, &quit])?;

    let light = taskbar_is_light();
    let size = tray_icon_size();
    let img = Image::new_owned(icons::render(IconState::Idle, size, light), size, size);
    let built = TrayIconBuilder::with_id(TRAY_ID)
        .icon(img)
        .tooltip(tooltip_for(Status::Idle, 0))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()))
        .on_tray_icon_event(|tray, ev| on_tray_event(tray.app_handle(), ev))
        .build(app);

    match built {
        Ok(tray) => {
            *lock(&shell.parts) = Some(TrayParts {
                tray,
                toggle,
                pause,
                auto,
                look: Some((IconState::Idle, light, size)),
                tip: tooltip_for(Status::Idle, 0),
                toggle_text: toggle_text(Status::Idle).into(),
                pause_text: pause_text(Status::Idle).into(),
                pause_enabled: false,
                toggle_enabled: true,
                auto_checked: prefs.launch_at_startup,
                light,
            });
            shell.tray_ok.store(true, Ordering::SeqCst);
        }
        Err(e) => eprintln!("Eco: no se pudo crear el ícono de la bandeja: {e}"),
    }

    // Ventanas auxiliares: el flyout se crea oculto para abrirse al instante.
    WebviewWindowBuilder::new(app, WIDGET, WebviewUrl::App("widget.html".into()))
        .title("Eco widget")
        .inner_size(WIDGET_SIZE.0, WIDGET_SIZE.1)
        .decorations(false)
        .shadow(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;

    // Ventana principal: oculta si arrancó con Windows (`--hidden`) y hay bandeja donde vivir.
    if let Some(main) = app.get_webview_window("main") {
        if !(launched_hidden() && shell.tray_ok.load(Ordering::SeqCst)) {
            let _ = main.show();
        }
    }

    spawn_updater(app.clone());
    Ok(())
}

fn spawn_updater(app: AppHandle) {
    let _ = std::thread::Builder::new().name("eco-tray".into()).spawn(move || {
        let mut n: u32 = 0;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            tick(&app, n % 8 == 0);
            n = n.wrapping_add(1);
        }
    });
}

/// Pone al día ícono, tooltip, menú y píldora con el estado actual. No llamar desde el hilo principal.
pub fn refresh(app: &AppHandle) {
    tick(app, true);
}

fn tick(app: &AppHandle, check_theme: bool) {
    let Some(shell) = app.try_state::<Shell>() else { return };
    let Some(engine) = app.try_state::<AppState>().map(|s| s.engine.clone()) else { return };
    let st = engine.state();
    let prefs = shell.prefs.get();
    {
        let mut g = lock(&shell.parts);
        if let Some(p) = g.as_mut() {
            if check_theme {
                p.light = taskbar_is_light();
            }
            let size = tray_icon_size();
            let want = (icon_state_for(st.status), p.light, size);
            if p.look != Some(want) {
                let img = Image::new_owned(icons::render(want.0, size, p.light), size, size);
                if p.tray.set_icon(Some(img)).is_ok() {
                    p.look = Some(want);
                }
            }
            let tip = tooltip_for(st.status, st.elapsed_ms);
            if tip != p.tip && p.tray.set_tooltip(Some(&tip)).is_ok() {
                p.tip = tip;
            }
            let t = toggle_text(st.status);
            if t != p.toggle_text && p.toggle.set_text(t).is_ok() {
                p.toggle_text = t.into();
            }
            let te = st.status != Status::Finalizing;
            if te != p.toggle_enabled && p.toggle.set_enabled(te).is_ok() {
                p.toggle_enabled = te;
            }
            let t = pause_text(st.status);
            if t != p.pause_text && p.pause.set_text(t).is_ok() {
                p.pause_text = t.into();
            }
            let pe = matches!(st.status, Status::Recording | Status::Paused);
            if pe != p.pause_enabled && p.pause.set_enabled(pe).is_ok() {
                p.pause_enabled = pe;
            }
            if p.auto_checked != prefs.launch_at_startup && p.auto.set_checked(prefs.launch_at_startup).is_ok() {
                p.auto_checked = prefs.launch_at_startup;
            }
        }
    }
    let want_pill = prefs.floating_pill && matches!(st.status, Status::Recording | Status::Paused);
    let have_pill = shell.pill_shown.load(Ordering::SeqCst);
    if want_pill && !have_pill {
        show_pill(app);
    } else if !want_pill && have_pill {
        hide_pill(app);
    }
}

/* ───────────────────────── eventos del ícono y del menú ───────────────────────── */

fn rect_of(r: &tauri::Rect) -> Rect {
    let p = r.position.to_physical::<i32>(1.0);
    let s = r.size.to_physical::<u32>(1.0);
    Rect::new(p.x, p.y, s.width as i32, s.height as i32)
}

fn on_tray_event(app: &AppHandle, ev: TrayIconEvent) {
    let shell = app.state::<Shell>();
    match ev {
        TrayIconEvent::Enter { rect, .. } | TrayIconEvent::Move { rect, .. } | TrayIconEvent::Leave { rect, .. } => {
            *lock(&shell.last_icon) = Some(rect_of(&rect));
        }
        TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } => {
            let r = rect_of(&rect);
            *lock(&shell.last_icon) = Some(r);
            left_click(app, Some(r));
        }
        TrayIconEvent::DoubleClick { button: MouseButton::Left, rect, .. } => {
            *lock(&shell.last_icon) = Some(rect_of(&rect));
            *lock(&shell.last_dbl) = Some(Instant::now());
            show_main(app);
        }
        _ => {}
    }
}

/// Acción del clic izquierdo: abre o cierra el flyout anclado al ícono.
pub fn left_click(app: &AppHandle, icon: Option<Rect>) {
    let shell = app.state::<Shell>();
    if lock(&shell.last_dbl).is_some_and(|t| t.elapsed() < DBLCLICK_GUARD) {
        return; // el segundo clic de un doble clic
    }
    let visible = app.get_webview_window(WIDGET).and_then(|w| w.is_visible().ok()).unwrap_or(false);
    if visible {
        hide_widget(app, false);
        return;
    }
    if lock(&shell.widget_blur_at).is_some_and(|t| t.elapsed() < BLUR_GUARD) {
        return; // este clic fue el que le quitó el foco: era un «cerrar»
    }
    show_widget(app, icon, false);
}

fn on_menu(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    // Los manejadores de menú corren en el hilo principal: el trabajo va a otro hilo.
    std::thread::spawn(move || match id.as_str() {
        "toggle" => toggle_record(&app),
        "pause" => toggle_pause(&app),
        "open" => show_main(&app),
        "folder" => {
            if let Err(e) = open_output_dir(&app) {
                emit_error(&app, e);
            }
        }
        "autostart" => {
            let shell = app.state::<Shell>();
            let want = !shell.prefs.get().launch_at_startup;
            let mut p = shell.prefs.get();
            p.launch_at_startup = want;
            if let Err(e) = apply_prefs(&app, &p) {
                emit_error(&app, e);
                refresh_auto_check(&app);
            }
        }
        "quit" => quit(&app),
        _ => {}
    });
}

fn refresh_auto_check(app: &AppHandle) {
    let shell = app.state::<Shell>();
    let on = app.autolaunch().is_enabled().unwrap_or_else(|_| shell.prefs.get().launch_at_startup);
    let mut g = lock(&shell.parts);
    if let Some(p) = g.as_mut() {
        if p.auto.set_checked(on).is_ok() {
            p.auto_checked = on;
        }
    }
}

fn emit_error(app: &AppHandle, message: String) {
    let _ = app.emit("engine-error", EngineErrorPayload { message, fatal: false });
}

pub fn toggle_record(app: &AppHandle) {
    let engine = app.state::<AppState>().engine.clone();
    let r = match engine.state().status {
        Status::Idle => engine.start_recording().map(|_| ()),
        Status::Recording | Status::Paused => engine.stop_recording().map(|_| ()),
        Status::Finalizing => Ok(()),
    };
    if let Err(e) = r {
        emit_error(app, e);
    }
    refresh(app);
}

pub fn toggle_pause(app: &AppHandle) {
    let engine = app.state::<AppState>().engine.clone();
    let r = match engine.state().status {
        Status::Recording => engine.pause_recording().map(|_| ()),
        Status::Paused => engine.resume_recording().map(|_| ()),
        _ => Ok(()),
    };
    if let Err(e) = r {
        emit_error(app, e);
    }
    refresh(app);
}

pub fn open_output_dir(app: &AppHandle) -> Result<(), String> {
    let dir = app.state::<AppState>().engine.settings().output_dir;
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear la carpeta «{dir}»: {e}"))?;
    app.opener().open_path(dir, None::<&str>).map_err(|e| format!("No se pudo abrir la carpeta: {e}"))
}

/// Detiene y guarda si está grabando, y cierra la app.
pub fn quit(app: &AppHandle) {
    app.state::<Shell>().quitting.store(true, Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        let engine = app.state::<AppState>().engine.clone();
        if matches!(engine.state().status, Status::Recording | Status::Paused) {
            let _ = engine.stop_recording();
        }
        app.exit(0);
    });
}

/* ───────────────────────── preferencias ───────────────────────── */

pub fn current_prefs(app: &AppHandle) -> AppPrefs {
    let shell = app.state::<Shell>();
    let mut p = shell.prefs.get();
    if let Ok(on) = app.autolaunch().is_enabled() {
        p.launch_at_startup = on;
    }
    p
}

/// Valida/aplica efectos (inicio con Windows), guarda y avisa a las ventanas.
pub fn apply_prefs(app: &AppHandle, incoming: &AppPrefs) -> Result<AppPrefs, String> {
    let shell = app.state::<Shell>();
    let cur = shell.prefs.get();
    let merged = prefs::merge_from_ui(&cur, incoming);
    let actual = app.autolaunch().is_enabled().unwrap_or(cur.launch_at_startup);
    if merged.launch_at_startup != actual {
        let m = app.autolaunch();
        let r = if merged.launch_at_startup { m.enable() } else { m.disable() };
        r.map_err(|e| format!("No se pudo {} el inicio con Windows: {e}", if merged.launch_at_startup { "activar" } else { "desactivar" }))?;
    }
    let saved = shell.prefs.update(|p| *p = merged)?;
    let _ = app.emit("app-prefs-changed", &saved);
    refresh(app);
    Ok(saved)
}

/* ───────────────────────── ventana principal ───────────────────────── */

pub fn show_main(app: &AppHandle) {
    hide_widget(app, false);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    let Some(shell) = app.try_state::<Shell>() else { return };
    match (window.label(), event) {
        ("main", WindowEvent::CloseRequested { api, .. }) => {
            if shell.quitting.load(Ordering::SeqCst) {
                return;
            }
            api.prevent_close();
            let prefs = shell.prefs.get();
            if prefs.close_to_tray && shell.tray_ok.load(Ordering::SeqCst) {
                let _ = window.hide();
                if !prefs.tray_hint_shown {
                    if let Ok(saved) = shell.prefs.update(|p| p.tray_hint_shown = true) {
                        let _ = app.emit("app-prefs-changed", &saved);
                    }
                    let app = app.clone();
                    std::thread::spawn(move || show_widget(&app, None, true));
                }
            } else {
                quit(app);
            }
        }
        (WIDGET, WindowEvent::Focused(false)) => hide_widget(app, true),
        (PILL, WindowEvent::Moved(pos)) => schedule_pill_save(app, *pos),
        _ => {}
    }
}

/* ───────────────────────── flyout ───────────────────────── */

struct Geo {
    monitor: Rect,
    work: Rect,
    scale: f64,
}

fn geometry(app: &AppHandle, at: Option<(i32, i32)>) -> Option<Geo> {
    let mon = at
        .and_then(|(x, y)| app.monitor_from_point(x as f64, y as f64).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let (p, s, w) = (mon.position(), mon.size(), mon.work_area());
    Some(Geo {
        monitor: Rect::new(p.x, p.y, s.width as i32, s.height as i32),
        work: Rect::new(w.position.x, w.position.y, w.size.width as i32, w.size.height as i32),
        scale: mon.scale_factor(),
    })
}

/// Posición física del flyout para un ícono dado (o la esquina de la barra si no se conoce).
fn widget_placement(app: &AppHandle, anchor: Option<Rect>) -> Option<((i32, i32), (i32, i32))> {
    let g = geometry(app, anchor.map(|r| r.center()))?;
    let icon = anchor.unwrap_or_else(|| flyout::fallback_icon(g.monitor, g.work, g.scale));
    let size = ((WIDGET_SIZE.0 * g.scale).round() as i32, (WIDGET_SIZE.1 * g.scale).round() as i32);
    let gap = (FLYOUT_GAP * g.scale).round() as i32;
    Some((flyout::flyout_position(icon, g.monitor, g.work, size, gap), size))
}

/// Coloca la ventana para que su área cliente (lo visible) quede exactamente en `pos` con `size`
/// (píxeles físicos). Las ventanas sin decoraciones de tao llevan un marco invisible y su modelo de
/// tamaño no coincide con el real, así que se mide con Win32 y se corrige en vez de fiarse de
/// `set_size`/`set_position`.
fn place_exact(win: &tauri::WebviewWindow, pos: (i32, i32), size: (i32, i32)) {
    let Ok(hwnd) = win.hwnd() else {
        let _ = win.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
        let _ = win.set_position(PhysicalPosition::new(pos.0, pos.1));
        return;
    };
    let h = hwnd.0 as isize;
    for _ in 0..3 {
        let (mut wr, mut cr, mut pt) = (sys::Rc::default(), sys::Rc::default(), sys::Pt::default());
        let ok = unsafe { sys::GetWindowRect(h, &mut wr) != 0 && sys::GetClientRect(h, &mut cr) != 0 && sys::ClientToScreen(h, &mut pt) != 0 };
        if !ok {
            break;
        }
        let (nc_w, nc_h) = ((wr.r - wr.l) - cr.r, (wr.b - wr.t) - cr.b);
        let (x, y) = (pos.0 - (pt.x - wr.l), pos.1 - (pt.y - wr.t));
        let (cx, cy) = (size.0 + nc_w, size.1 + nc_h);
        if (x, y, cx, cy) == (wr.l, wr.t, wr.r - wr.l, wr.b - wr.t) {
            break;
        }
        unsafe {
            sys::SetWindowPos(h, 0, x, y, cx, cy, sys::SWP_NOZORDER | sys::SWP_NOACTIVATE);
        }
    }
}

pub fn show_widget(app: &AppHandle, anchor: Option<Rect>, hint: bool) {
    let Some(win) = app.get_webview_window(WIDGET) else { return };
    let shell = app.state::<Shell>();
    let anchor = anchor.or(*lock(&shell.last_icon));
    if let Some((pos, size)) = widget_placement(app, anchor) {
        place_exact(&win, pos, size);
    }
    *lock(&shell.widget_blur_at) = None;
    let gen = shell.widget_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit_to(WIDGET, "widget-shown", serde_json::json!({ "hint": hint }));
    let _ = win.show();
    let _ = win.set_focus();
    if hint {
        // Aviso pasajero: se retira solo si nadie lo tocó (otro `show_widget` cambia `gen`).
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(HINT_MS));
            if app.state::<Shell>().widget_gen.load(Ordering::SeqCst) == gen {
                hide_widget(&app, false);
            }
        });
    }
}

pub fn hide_widget(app: &AppHandle, from_blur: bool) {
    let Some(win) = app.get_webview_window(WIDGET) else { return };
    if from_blur {
        *lock(&app.state::<Shell>().widget_blur_at) = Some(Instant::now());
    }
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
    }
}

/* ───────────────────────── píldora flotante ───────────────────────── */

fn ensure_pill(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(PILL) {
        return Some(w);
    }
    WebviewWindowBuilder::new(app, PILL, WebviewUrl::App("pill.html".into()))
        .title("Eco pill")
        .inner_size(PILL_SIZE.0, PILL_SIZE.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .focusable(false)
        .build()
        .ok()
}

fn pill_position(app: &AppHandle, prefs: &AppPrefs) -> Option<(i32, i32, (i32, i32))> {
    if let (Some(x), Some(y)) = (prefs.pill_x, prefs.pill_y) {
        if let Some(g) = geometry(app, Some((x + 16, y + 16))) {
            if app.monitor_from_point((x + 16) as f64, (y + 16) as f64).ok().flatten().is_some() {
                let size = ((PILL_SIZE.0 * g.scale).round() as i32, (PILL_SIZE.1 * g.scale).round() as i32);
                return Some((x, y, size));
            }
        }
    }
    let g = geometry(app, None)?;
    let size = ((PILL_SIZE.0 * g.scale).round() as i32, (PILL_SIZE.1 * g.scale).round() as i32);
    let x = g.work.x + (g.work.w - size.0) / 2;
    let y = g.work.y + (12.0 * g.scale).round() as i32;
    Some((x, y, size))
}

fn show_pill(app: &AppHandle) {
    let shell = app.state::<Shell>();
    let Some(win) = ensure_pill(app) else { return };
    let prefs = shell.prefs.get();
    *lock(&shell.pill_ignore_move_until) = Instant::now() + Duration::from_millis(900);
    if let Some((x, y, size)) = pill_position(app, &prefs) {
        place_exact(&win, (x, y), size);
    }
    let _ = win.show();
    shell.pill_shown.store(true, Ordering::SeqCst);
}

fn hide_pill(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PILL) {
        let _ = w.hide();
    }
    app.state::<Shell>().pill_shown.store(false, Ordering::SeqCst);
}

fn schedule_pill_save(app: &AppHandle, pos: PhysicalPosition<i32>) {
    let shell = app.state::<Shell>();
    if Instant::now() < *lock(&shell.pill_ignore_move_until) {
        return;
    }
    let gen = shell.pill_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        let shell = app.state::<Shell>();
        if shell.pill_gen.load(Ordering::SeqCst) == gen {
            // Se guarda la posición del área visible (la misma que usa `place_exact` al restaurar).
            let at = app.get_webview_window(PILL).and_then(|w| w.inner_position().ok()).unwrap_or(pos);
            let _ = shell.prefs.update(|p| {
                p.pill_x = Some(at.x);
                p.pill_y = Some(at.y);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_texts() {
        assert_eq!(tooltip_for(Status::Idle, 0), "Eco — Listo");
        assert_eq!(tooltip_for(Status::Recording, 133_400), "Eco — Grabando 02:13");
        assert_eq!(tooltip_for(Status::Paused, 133_999), "Eco — En pausa 02:13");
        assert_eq!(tooltip_for(Status::Recording, 3_725_000), "Eco — Grabando 1:02:05");
    }

    #[test]
    fn menu_texts_follow_state() {
        assert_eq!(toggle_text(Status::Idle), "Iniciar grabación");
        assert_eq!(toggle_text(Status::Recording), "Detener grabación");
        assert_eq!(toggle_text(Status::Paused), "Detener grabación");
        assert_eq!(pause_text(Status::Recording), "Pausar");
        assert_eq!(pause_text(Status::Paused), "Reanudar");
    }

    #[test]
    fn icon_variant_per_status() {
        assert_eq!(icon_state_for(Status::Idle), IconState::Idle);
        assert_eq!(icon_state_for(Status::Finalizing), IconState::Idle);
        assert_eq!(icon_state_for(Status::Recording), IconState::Recording);
        assert_eq!(icon_state_for(Status::Paused), IconState::Paused);
    }

    /// Vista previa de los íconos: `ECO_ICON_DUMP=<carpeta> cargo test dump_icon_previews`.
    #[test]
    fn dump_icon_previews() {
        let Some(dir) = std::env::var_os("ECO_ICON_DUMP") else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, st) in [("idle", IconState::Idle), ("rec", IconState::Recording), ("pause", IconState::Paused)] {
            for size in [16u32, 20, 24, 32] {
                for (bar, light) in [("dark", false), ("light", true)] {
                    std::fs::write(dir.join(format!("{name}-{size}-{bar}.rgba")), icons::render(st, size, light)).unwrap();
                }
            }
        }
    }
}
