//! Tipos del contrato backend ↔ UI. En el cable todo va en camelCase.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
    /// Solo en el pseudo-dispositivo `default`: nombre real al que apunta hoy (sin el prefijo «Predeterminado…»).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Devices {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    #[serde(rename = "separate")]
    Separate,
    #[serde(rename = "mixed")]
    Mixed,
    #[serde(rename = "both")]
    Both,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    #[serde(rename = "mp3")]
    Mp3,
    #[serde(rename = "wav16")]
    Wav16,
    #[serde(rename = "wav24")]
    Wav24,
    #[serde(rename = "wav32f")]
    Wav32f,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "dark")]
    Dark,
    #[serde(rename = "light")]
    Light,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub input_id: String,
    pub output_id: String,
    pub capture_input: bool,
    pub capture_output: bool,
    pub mode: Mode,
    pub format: Format,
    pub output_dir: String,
    /// Ganancia del micrófono en dB (−12…+30). Se aplica antes del AGC.
    pub input_gain_db: f32,
    /// Ganancia del audio del sistema en dB (−12…+12).
    pub output_gain_db: f32,
    /// «Voz automática»: control automático de ganancia del micrófono (objetivo ≈ −18 dBFS RMS).
    pub input_agc: bool,
    /// En la mezcla, baja ~8 dB el audio del sistema mientras hay voz en el micrófono.
    pub mix_voice_priority: bool,
    /// Solo lectura de ajustes antiguos (`inputGain` lineal). `validated()` lo migra a dB y lo borra.
    #[serde(rename = "inputGain", default, skip_serializing)]
    pub legacy_input_gain: Option<f32>,
    /// Solo lectura de ajustes antiguos (`outputGain` lineal).
    #[serde(rename = "outputGain", default, skip_serializing)]
    pub legacy_output_gain: Option<f32>,
    pub monitor_when_idle: bool,
    pub theme: Theme,
    /// Atajo global iniciar/detener (formato del plugin, p. ej. `Ctrl+Alt+R`).
    pub hotkey_toggle: String,
    /// Atajo global pausar/reanudar.
    pub hotkey_pause: String,
}

pub const INPUT_GAIN_DB_MIN: f32 = -12.0;
pub const INPUT_GAIN_DB_MAX: f32 = 30.0;
pub const OUTPUT_GAIN_DB_MIN: f32 = -12.0;
pub const OUTPUT_GAIN_DB_MAX: f32 = 12.0;
pub const DEFAULT_HOTKEY_TOGGLE: &str = "Ctrl+Alt+R";
pub const DEFAULT_HOTKEY_PAUSE: &str = "Ctrl+Alt+P";

impl Default for Settings {
    fn default() -> Self {
        Settings {
            input_id: "default".into(),
            output_id: "default".into(),
            capture_input: true,
            capture_output: false,
            mode: Mode::Separate,
            format: Format::Mp3,
            output_dir: default_output_dir(),
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            input_agc: true,
            mix_voice_priority: true,
            legacy_input_gain: None,
            legacy_output_gain: None,
            monitor_when_idle: false,
            theme: Theme::System,
            hotkey_toggle: DEFAULT_HOTKEY_TOGGLE.into(),
            hotkey_pause: DEFAULT_HOTKEY_PAUSE.into(),
        }
    }
}

/// Normaliza una ruta al estilo Windows: la barra normal pasa a barra invertida, se colapsan los
/// separadores repetidos (respetando los prefijos de red y de ruta larga) y se quita el separador final salvo en la raíz.
pub fn win_path(p: &str) -> String {
    const SEP: char = '\u{5c}';
    let sep = SEP.to_string();
    let t = p.trim().replace('/', &sep);
    let unc_long = format!("{sep}{sep}?{sep}");
    let unc = format!("{sep}{sep}");
    let (prefix, rest) = if let Some(r) = t.strip_prefix(unc_long.as_str()) {
        (unc_long.as_str(), r.to_string())
    } else if let Some(r) = t.strip_prefix(unc.as_str()) {
        (unc.as_str(), r.to_string())
    } else {
        ("", t.clone())
    };
    let parts: Vec<&str> = rest.split(SEP).filter(|x| !x.is_empty()).collect();
    let mut out = format!("{prefix}{}", parts.join(&sep));
    if prefix.is_empty() && parts.len() == 1 && parts[0].len() == 2 && parts[0].ends_with(':') {
        out.push(SEP); // «C:» pasa a «C:» + separador
    }
    if prefix.is_empty() && rest.starts_with(SEP) {
        out.insert(0, SEP); // raíz de la unidad actual
    }
    out
}

/// `%USERPROFILE%\Music\Eco`
pub fn default_output_dir() -> String {
    #[cfg(windows)] unsafe {
        use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Desktop, KF_FLAG_DEFAULT};
        use windows::Win32::System::Com::CoTaskMemFree;
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None) {
            let desktop = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            if let Some(desktop) = desktop { return desktop; }
        }
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home)
        .join("Desktop")
        .to_string_lossy()
        .into_owned()
}

impl Settings {
    /// The output layout depends only on the authoritative start settings.
    pub fn direct_mp3(&self) -> bool {
        self.format == Format::Mp3 && self.capture_input != self.capture_output
    }

    /// Normaliza valores fuera de rango o vacíos.
    pub fn validated(mut self) -> Settings {
        fn db(g: f32, lo: f32, hi: f32) -> f32 {
            if g.is_finite() {
                g.clamp(lo, hi)
            } else {
                0.0
            }
        }
        // Ajustes antiguos: ganancia lineal → dB (0 = silencio → mínimo).
        fn lin_to_db(g: f32) -> f32 {
            if g.is_finite() && g > 0.0 {
                20.0 * g.log10()
            } else if g.is_finite() {
                -120.0
            } else {
                0.0
            }
        }
        if let Some(g) = self.legacy_input_gain.take() {
            self.input_gain_db = lin_to_db(g);
        }
        if let Some(g) = self.legacy_output_gain.take() {
            self.output_gain_db = lin_to_db(g);
        }
        self.input_gain_db = db(self.input_gain_db, INPUT_GAIN_DB_MIN, INPUT_GAIN_DB_MAX);
        self.output_gain_db = db(self.output_gain_db, OUTPUT_GAIN_DB_MIN, OUTPUT_GAIN_DB_MAX);
        if self.input_id.trim().is_empty() {
            self.input_id = "default".into();
        }
        if self.output_id.trim().is_empty() {
            self.output_id = "default".into();
        }
        if self.output_dir.trim().is_empty() {
            self.output_dir = default_output_dir();
        }
        self.output_dir = win_path(&self.output_dir);
        self.hotkey_toggle = self.hotkey_toggle.trim().to_string();
        self.hotkey_pause = self.hotkey_pause.trim().to_string();
        if self.hotkey_toggle.is_empty() {
            self.hotkey_toggle = DEFAULT_HOTKEY_TOGGLE.into();
        }
        if self.hotkey_pause.is_empty() {
            self.hotkey_pause = DEFAULT_HOTKEY_PAUSE.into();
        }
        self
    }

    /// Rechaza carpetas de salida imposibles (caracteres ilegales en Windows o ruta que ya es un archivo).
    /// No crea nada: la carpeta se crea al grabar.
    pub fn check_output_dir(&self) -> Result<(), String> {
        let d = self.output_dir.as_str();
        let body = d.strip_prefix(r"\\?\").unwrap_or(d);
        // «:» solo es válido como separador de unidad (X:).
        let bad = body.char_indices().any(|(i, c)| matches!(c, '<' | '>' | '"' | '|' | '?' | '*') || (c == ':' && i != 1) || c.is_control());
        if bad {
            return Err(format!("La carpeta «{d}» no es válida: contiene caracteres que Windows no permite."));
        }
        let p = std::path::Path::new(d);
        if p.exists() && !p.is_dir() {
            return Err(format!("«{d}» es un archivo, no una carpeta."));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "output")]
    Output,
    #[serde(rename = "mix")]
    Mix,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackFile {
    pub kind: TrackKind,
    pub path: String,
    pub size_bytes: u64,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub duration_ms: u64,
    pub files: Vec<TrackFile>,
    pub folder: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "recording")]
    Recording,
    #[serde(rename = "paused")]
    Paused,
    #[serde(rename = "finalizing")]
    Finalizing,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub status: Status,
    pub elapsed_ms: u64,
    pub recording_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LevelMeter {
    pub rms: f32,
    pub peak: f32,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Levels {
    pub input: LevelMeter,
    pub output: LevelMeter,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineErrorPayload {
    pub message: String,
    pub fatal: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyPayload {
    pub action: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_and_wire_names() {
        let s = Settings::default();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["inputId"], "default");
        assert_eq!(v["mode"], "separate");
        assert_eq!(v["format"], "mp3");
        assert_eq!(v["captureInput"], true);
        assert_eq!(v["captureOutput"], false);
        assert_eq!(v["monitorWhenIdle"], false);
        assert!(s.direct_mp3());
        assert_eq!(v["theme"], "system");
        assert_eq!(v["outputDir"].as_str().unwrap(), default_output_dir());
        // JSON parcial → defaults
        let p: Settings = serde_json::from_str(r#"{"format":"wav32f"}"#).unwrap();
        assert_eq!(p.format, Format::Wav32f);
        assert_eq!(p.mode, Mode::Separate);
        let legacy: Settings = serde_json::from_str(r#"{"format":"wav24","captureInput":true,"captureOutput":true,"mode":"both"}"#).unwrap();
        assert_eq!(legacy.format, Format::Wav24);
        assert!(legacy.capture_output);
        assert_eq!(legacy.mode, Mode::Both);
        assert!(!legacy.direct_mp3());
    }

    #[test]
    fn settings_validation_clamps() {
        let s = Settings {
            input_gain_db: 99.0,
            output_gain_db: f32::NAN,
            input_id: "  ".into(),
            ..Settings::default()
        };
        let v = s.validated();
        assert_eq!(v.input_gain_db, 30.0);
        assert_eq!(v.output_gain_db, 0.0);
        assert_eq!(v.input_id, "default");
        let lo = Settings { input_gain_db: -50.0, output_gain_db: 40.0, ..Settings::default() }.validated();
        assert_eq!((lo.input_gain_db, lo.output_gain_db), (-12.0, 12.0));
    }

    #[test]
    fn old_linear_gains_migrate_to_db() {
        let s: Settings = serde_json::from_str(r#"{"inputGain":1.0,"outputGain":0.5,"format":"wav16"}"#).unwrap();
        let v = s.validated();
        assert_eq!(v.input_gain_db, 0.0);
        assert!((v.output_gain_db + 6.02).abs() < 0.01, "{}", v.output_gain_db);
        assert!(v.input_agc && v.mix_voice_priority);
        assert_eq!(v.format, Format::Wav16);
        let two: Settings = serde_json::from_str(r#"{"inputGain":2.0}"#).unwrap();
        assert!((two.validated().input_gain_db - 6.02).abs() < 0.01);
        // Los campos nuevos se serializan en camelCase y el antiguo ya no sale.
        let j = serde_json::to_string(&v).unwrap();
        assert!(j.contains("\"inputGainDb\":0.0") && j.contains("\"inputAgc\":true") && j.contains("\"mixVoicePriority\":true"));
        assert!(!j.contains("\"inputGain\""), "{j}");
        // Un archivo nuevo con dB no se toca.
        let n: Settings = serde_json::from_str(r#"{"inputGainDb":18.0,"inputAgc":false}"#).unwrap();
        let n = n.validated();
        assert_eq!((n.input_gain_db, n.input_agc), (18.0, false));
    }

    #[test]
    fn output_dir_check() {
        let mk = |d: &str| Settings { output_dir: d.into(), ..Settings::default() };
        assert!(mk("C:/Users/x/Music/Eco").check_output_dir().is_ok());
        assert!(mk(r"D:\Grabaciones ñ\Eco").check_output_dir().is_ok());
        assert!(mk("C:/a<b>|c?").check_output_dir().is_err());
        assert!(mk("C:/a:b").check_output_dir().is_err());
        assert!(mk("C:/Windows/System32/drivers/etc/hosts").check_output_dir().is_err());
    }

    #[test]
    fn device_real_name_is_optional_on_the_wire() {
        let d: Device = serde_json::from_str(r#"{"id":"a","name":"a","isDefault":false,"sampleRate":1,"channels":1}"#).unwrap();
        assert_eq!(d.real_name, None);
        assert!(serde_json::to_value(&d).unwrap().get("realName").is_none());
        let d2 = Device { real_name: Some("Mic".into()), ..d };
        assert_eq!(serde_json::to_value(&d2).unwrap()["realName"], "Mic");
    }

    #[test]
    fn win_path_normalizes_separators() {
        let b = '\u{5c}';
        let j = |parts: &[&str]| parts.join(&b.to_string());
        assert_eq!(win_path("C:/Users/usuario/Music/Eco"), j(&["C:", "Users", "usuario", "Music", "Eco"]));
        assert_eq!(win_path("C:/a\\b//c/"), j(&["C:", "a", "b", "c"]));
        assert_eq!(win_path("C:"), format!("C:{b}"));
        assert_eq!(win_path("C:/"), format!("C:{b}"));
        let unc = format!("{b}{b}srv{b}share{b}x");
        assert_eq!(win_path("//srv/share/x/"), unc);
        assert_eq!(win_path(&unc), unc);
        assert!(!win_path("D:/x/y/z.wav").contains('/'));
        // validated() también la aplica al guardar.
        let s = Settings { output_dir: "C:/Users/x/Music/Eco/".into(), ..Settings::default() }.validated();
        assert_eq!(s.output_dir, j(&["C:", "Users", "x", "Music", "Eco"]));
    }

    #[test]
    fn hotkeys_default_and_old_settings_compat() {
        let s = Settings::default();
        assert_eq!((s.hotkey_toggle.as_str(), s.hotkey_pause.as_str()), ("Ctrl+Alt+R", "Ctrl+Alt+P"));
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["hotkeyToggle"], "Ctrl+Alt+R");
        assert_eq!(v["hotkeyPause"], "Ctrl+Alt+P");
        // settings.json de una versión anterior (sin los campos) sigue cargando con los defaults.
        let old: Settings = serde_json::from_str(r#"{"mode":"mixed","outputDir":"C:/x"}"#).unwrap();
        assert_eq!(old.hotkey_toggle, "Ctrl+Alt+R");
        let blank = Settings { hotkey_toggle: "  ".into(), hotkey_pause: String::new(), ..Settings::default() }.validated();
        assert_eq!((blank.hotkey_toggle.as_str(), blank.hotkey_pause.as_str()), ("Ctrl+Alt+R", "Ctrl+Alt+P"));
    }

    #[test]
    fn state_wire_format() {
        let st = EngineState {
            status: Status::Recording,
            elapsed_ms: 5,
            recording_id: None,
        };
        let v = serde_json::to_value(&st).unwrap();
        assert_eq!(v["status"], "recording");
        assert_eq!(v["elapsedMs"], 5);
        assert!(v["recordingId"].is_null());
    }
}
