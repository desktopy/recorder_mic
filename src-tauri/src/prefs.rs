//! Preferencias de la app (ventana y bandeja), aparte de los `Settings` del motor.
//! Se guardan en `%APPDATA%\Eco\app-prefs.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppPrefs {
    /// Cerrar la ventana principal la deja en la bandeja en vez de salir.
    pub close_to_tray: bool,
    /// Arrancar con Windows (entrada `Run` con `--hidden`). El valor real lo manda el registro.
    pub launch_at_startup: bool,
    /// Píldora flotante mientras se graba.
    pub floating_pill: bool,
    /// Ya se avisó una vez que Eco sigue en la bandeja.
    pub tray_hint_shown: bool,
    /// Última posición de la píldora (píxeles físicos). Las gestiona el backend.
    pub pill_x: Option<i32>,
    pub pill_y: Option<i32>,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            launch_at_startup: false,
            floating_pill: false,
            tray_hint_shown: false,
            pill_x: None,
            pill_y: None,
        }
    }
}

pub fn prefs_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Roaming")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Eco").join("app-prefs.json")
}

/// Lee las preferencias; archivo ausente o corrupto → defaults.
pub fn load_from(path: &Path) -> AppPrefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &Path, prefs: &AppPrefs) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear la carpeta de preferencias: {e}"))?;
    }
    let json = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("No se pudieron guardar las preferencias: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("No se pudieron guardar las preferencias: {e}"))
}

/// Copia en memoria + persistencia.
pub struct PrefsStore {
    path: PathBuf,
    cur: Mutex<AppPrefs>,
}

impl PrefsStore {
    pub fn load() -> Self {
        let path = prefs_path();
        let cur = Mutex::new(load_from(&path));
        Self { path, cur }
    }

    pub fn get(&self) -> AppPrefs {
        self.cur.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Aplica `f` y guarda. Devuelve las preferencias resultantes.
    pub fn update(&self, f: impl FnOnce(&mut AppPrefs)) -> Result<AppPrefs, String> {
        let mut g = self.cur.lock().unwrap_or_else(|p| p.into_inner());
        let mut next = g.clone();
        f(&mut next);
        if next != *g {
            save_to(&self.path, &next)?;
            *g = next;
        }
        Ok(g.clone())
    }
}

/// Fusiona lo que manda la UI con lo guardado: la posición de la píldora es del backend y el aviso de
/// la bandeja no se vuelve a armar desde una copia vieja de la UI.
pub fn merge_from_ui(current: &AppPrefs, incoming: &AppPrefs) -> AppPrefs {
    AppPrefs {
        close_to_tray: incoming.close_to_tray,
        launch_at_startup: incoming.launch_at_startup,
        floating_pill: incoming.floating_pill,
        tray_hint_shown: current.tray_hint_shown || incoming.tray_hint_shown,
        pill_x: current.pill_x,
        pill_y: current.pill_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eco-prefs-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("app-prefs.json")
    }

    #[test]
    fn defaults_are_as_specified() {
        let d = AppPrefs::default();
        assert!(d.close_to_tray);
        assert!(!d.launch_at_startup);
        assert!(!d.floating_pill);
        assert!(!d.tray_hint_shown);
        assert_eq!((d.pill_x, d.pill_y), (None, None));
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(load_from(&tmp("missing")), AppPrefs::default());
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let p = tmp("corrupt");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "{ esto no es json").unwrap();
        assert_eq!(load_from(&p), AppPrefs::default());
    }

    #[test]
    fn partial_file_fills_missing_fields_with_defaults() {
        let p = tmp("partial");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, r#"{"floatingPill": true}"#).unwrap();
        let l = load_from(&p);
        assert!(l.floating_pill);
        assert!(l.close_to_tray, "closeToTray debe conservar su default true");
        assert!(!l.launch_at_startup);
    }

    #[test]
    fn camel_case_on_the_wire_and_roundtrip() {
        let p = tmp("rt");
        let prefs = AppPrefs { launch_at_startup: true, tray_hint_shown: true, pill_x: Some(-10), pill_y: Some(40), ..Default::default() };
        save_to(&p, &prefs).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        for k in ["closeToTray", "launchAtStartup", "floatingPill", "trayHintShown"] {
            assert!(text.contains(k), "falta {k} en {text}");
        }
        assert_eq!(load_from(&p), prefs);
    }

    #[test]
    fn store_update_persists_only_on_change() {
        let p = tmp("store");
        let store = PrefsStore { path: p.clone(), cur: Mutex::new(AppPrefs::default()) };
        store.update(|_| {}).unwrap();
        assert!(!p.exists(), "sin cambios no se escribe");
        let r = store.update(|x| x.floating_pill = true).unwrap();
        assert!(r.floating_pill);
        assert!(load_from(&p).floating_pill);
    }

    #[test]
    fn merge_keeps_backend_owned_fields() {
        let cur = AppPrefs { tray_hint_shown: true, pill_x: Some(5), pill_y: Some(6), ..Default::default() };
        let inc = AppPrefs { close_to_tray: false, floating_pill: true, tray_hint_shown: false, pill_x: Some(99), pill_y: None, ..Default::default() };
        let m = merge_from_ui(&cur, &inc);
        assert!(!m.close_to_tray && m.floating_pill);
        assert!(m.tray_hint_shown);
        assert_eq!((m.pill_x, m.pill_y), (Some(5), Some(6)));
    }
}
