//! Atajos globales configurables: validación, registro con rollback y estado para la UI.

use crate::commands::AppState;
use eco_audio::types::{Settings, DEFAULT_HOTKEY_PAUSE, DEFAULT_HOTKEY_TOGGLE};
use serde::Serialize;
use std::str::FromStr;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Un atajo: texto (formato del plugin), su forma parseada y si el sistema lo aceptó.
#[derive(Clone, Debug)]
pub struct Slot {
    pub accel: String,
    pub shortcut: Option<Shortcut>,
    pub ok: bool,
}

#[derive(Debug)]
pub struct Hotkeys {
    pub toggle: Slot,
    pub pause: Slot,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlotStatus {
    pub accel: String,
    pub ok: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub toggle: SlotStatus,
    pub pause: SlotStatus,
}

/// Parsea y valida un atajo: formato del plugin y al menos un modificador.
pub fn parse_accel(accel: &str) -> Result<Shortcut, String> {
    let a = accel.trim();
    if a.is_empty() {
        return Err("El atajo está vacío.".into());
    }
    let sc = Shortcut::from_str(a).map_err(|_| format!("El atajo «{a}» no es válido."))?;
    if sc.mods.is_empty() {
        return Err(format!("El atajo «{a}» necesita al menos un modificador (Ctrl, Alt, Mayús o Win)."));
    }
    Ok(sc)
}

/// Valida el par: ambos válidos y distintos entre sí.
pub fn check_pair(toggle: &str, pause: &str) -> Result<(Shortcut, Shortcut), String> {
    let t = parse_accel(toggle)?;
    let p = parse_accel(pause)?;
    if t == p {
        return Err("Los atajos de iniciar/detener y de pausar no pueden ser el mismo.".into());
    }
    Ok((t, p))
}

/// Si lo guardado en settings.json no es válido (editado a mano), vuelve a los defaults.
pub fn sanitize(mut s: Settings) -> Settings {
    if check_pair(&s.hotkey_toggle, &s.hotkey_pause).is_err() {
        s.hotkey_toggle = DEFAULT_HOTKEY_TOGGLE.into();
        s.hotkey_pause = DEFAULT_HOTKEY_PAUSE.into();
    }
    s
}

fn describe(e: &str) -> String {
    if e.contains("already registered") {
        "otra aplicación ya lo está usando".to_string()
    } else {
        e.to_string()
    }
}

fn empty_slot(accel: &str) -> Slot {
    Slot { accel: accel.to_string(), shortcut: parse_accel(accel).ok(), ok: false }
}

impl Hotkeys {
    pub fn new(s: &Settings) -> Hotkeys {
        Hotkeys { toggle: empty_slot(&s.hotkey_toggle), pause: empty_slot(&s.hotkey_pause) }
    }

    pub fn status(&self) -> HotkeyStatus {
        HotkeyStatus {
            toggle: SlotStatus { accel: self.toggle.accel.clone(), ok: self.toggle.ok },
            pause: SlotStatus { accel: self.pause.accel.clone(), ok: self.pause.ok },
        }
    }

    /// `"toggle"` / `"pause"` según el atajo pulsado.
    pub fn action_for(&self, sc: &Shortcut) -> Option<&'static str> {
        if self.toggle.ok && self.toggle.shortcut.as_ref() == Some(sc) {
            Some("toggle")
        } else if self.pause.ok && self.pause.shortcut.as_ref() == Some(sc) {
            Some("pause")
        } else {
            None
        }
    }
}

/// Registra los atajos al arrancar. No falla: deja `ok=false` en los que estén ocupados.
pub fn register_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut hk = state.hotkeys.lock().unwrap_or_else(|p| p.into_inner());
    let hk = &mut *hk;
    for slot in [&mut hk.toggle, &mut hk.pause] {
        if let Some(sc) = slot.shortcut {
            slot.ok = app.global_shortcut().register(sc).is_ok();
        }
    }
}

/// Cambia los atajos registrados. Si un atajo *nuevo* no se puede registrar devuelve `Err` en español
/// y deja registrado el anterior. Un atajo sin cambios que estaba ocupado se reintenta sin error.
pub fn apply(app: &AppHandle, toggle: &str, pause: &str) -> Result<(), String> {
    let (new_t, new_p) = check_pair(toggle, pause)?;
    let state = app.state::<AppState>();
    let mut hk = state.hotkeys.lock().unwrap_or_else(|p| p.into_inner());
    let gs = app.global_shortcut();

    let old = [hk.toggle.clone(), hk.pause.clone()];
    let new = [(toggle.trim().to_string(), new_t), (pause.trim().to_string(), new_p)];
    // 1) Libera los viejos que cambian (permite intercambiar un atajo por el otro).
    let mut released = Vec::new();
    for (o, (_, n)) in old.iter().zip(&new) {
        if o.ok && o.shortcut.as_ref() != Some(n) {
            if let Some(sc) = o.shortcut {
                let _ = gs.unregister(sc);
                released.push(sc);
            }
        }
    }
    // 2) Registra los nuevos.
    let mut results: Vec<Slot> = Vec::new();
    let mut registered_new: Vec<Shortcut> = Vec::new();
    let mut failure: Option<String> = None;
    for (o, (accel, n)) in old.iter().zip(&new) {
        let same = o.shortcut.as_ref() == Some(n);
        if same && o.ok {
            results.push(o.clone());
            continue;
        }
        match gs.register(*n) {
            Ok(()) => {
                results.push(Slot { accel: accel.clone(), shortcut: Some(*n), ok: true });
                if !same {
                    registered_new.push(*n);
                }
            }
            // Sin cambios y sigue ocupado: no es un error de este guardado.
            Err(_) if same => results.push(Slot { accel: accel.clone(), shortcut: Some(*n), ok: false }),
            Err(e) => {
                failure.get_or_insert(format!(
                    "No se pudo registrar el atajo {accel}: {}. Se conserva el anterior.",
                    describe(&e.to_string())
                ));
                results.push(o.clone());
            }
        }
    }
    if let Some(msg) = failure {
        // Rollback: quita lo que se registró de nuevo y restaura los viejos liberados.
        for sc in registered_new {
            let _ = gs.unregister(sc);
        }
        for sc in released {
            let _ = gs.register(sc);
        }
        return Err(msg);
    }
    hk.toggle = results[0].clone();
    hk.pause = results[1].clone();
    Ok(())
}

pub fn status(app: &AppHandle) -> HotkeyStatus {
    let state = app.state::<AppState>();
    let hk = state.hotkeys.lock().unwrap_or_else(|p| p.into_inner());
    hk.status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates_accelerators() {
        assert!(parse_accel("Ctrl+Alt+R").is_ok());
        assert!(parse_accel("Ctrl+Alt+Shift+R").is_ok());
        assert_eq!(parse_accel("ctrl+alt+r").unwrap(), parse_accel("Ctrl+Alt+R").unwrap());
        assert!(parse_accel("Alt+F9").is_ok());
        // Sin modificador, solo modificadores, vacío o basura: error en español.
        for bad in ["R", "F9", "Ctrl+Alt", "", "   ", "Ctrl+Alt+Nada", "Ctrl++"] {
            let e = parse_accel(bad).unwrap_err();
            assert!(e.contains("atajo"), "{bad:?} -> {e}");
        }
        assert!(parse_accel("R").unwrap_err().contains("modificador"));
        // El par debe ser distinto aunque se escriba diferente.
        assert!(check_pair("Ctrl+Alt+R", "ctrl+alt+r").is_err());
        assert!(check_pair("Ctrl+Alt+Shift+R", "Ctrl+Alt+P").is_ok());
    }

    #[test]
    fn sanitize_falls_back_to_defaults() {
        let s = Settings { hotkey_toggle: "R".into(), ..Settings::default() };
        let s = sanitize(s);
        assert_eq!((s.hotkey_toggle.as_str(), s.hotkey_pause.as_str()), ("Ctrl+Alt+R", "Ctrl+Alt+P"));
        let ok = Settings { hotkey_toggle: "Ctrl+Alt+Shift+R".into(), ..Settings::default() };
        assert_eq!(sanitize(ok).hotkey_toggle, "Ctrl+Alt+Shift+R");
    }

    #[test]
    fn action_lookup_uses_registered_slots() {
        let s = Settings::default();
        let mut hk = Hotkeys::new(&s);
        let t = parse_accel("Ctrl+Alt+R").unwrap();
        assert_eq!(hk.action_for(&t), None); // aún no registrado
        hk.toggle.ok = true;
        assert_eq!(hk.action_for(&t), Some("toggle"));
        assert_eq!(hk.status().toggle, SlotStatus { accel: "Ctrl+Alt+R".into(), ok: true });
    }
}
