//! Persistencia de ajustes en `%APPDATA%\Eco\settings.json`.

use crate::types::Settings;
use std::path::PathBuf;

pub fn settings_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("Eco").join("settings.json")
}

/// Carga los ajustes; si no existen o están corruptos devuelve los defaults.
pub fn load() -> Settings {
    load_from(&settings_path())
}

pub fn load_from(path: &std::path::Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default()
        .validated()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    save_to(&settings_path(), settings)
}

pub fn save_to(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("No se pudo crear la carpeta de ajustes: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    // Escritura atómica: temporal + renombrar.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("No se pudieron guardar los ajustes: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("No se pudieron guardar los ajustes: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_corrupt_fallback() {
        let dir = std::env::temp_dir().join("eco-settings-test");
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("sub").join("settings.json");
        let s = Settings { input_gain_db: 12.0, ..Settings::default() };
        save_to(&p, &s).unwrap();
        assert_eq!(load_from(&p).input_gain_db, 12.0);
        std::fs::write(&p, "{ no es json").unwrap();
        assert_eq!(load_from(&p), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_file_with_linear_gain_loads_as_db() {
        let dir = std::env::temp_dir().join("eco-settings-migrate-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("settings.json");
        std::fs::write(&p, r#"{"inputId":"default","inputGain":1.0,"outputGain":1.0,"format":"wav24"}"#).unwrap();
        let s = load_from(&p);
        assert_eq!(s.input_gain_db, 0.0);
        assert_eq!(s.output_gain_db, 0.0);
        assert!(s.input_agc && s.mix_voice_priority);
        std::fs::write(&p, r#"{"inputGain":2.0}"#).unwrap();
        assert!((load_from(&p).input_gain_db - 6.0206).abs() < 0.01);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
