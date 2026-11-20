//! Sesiones en disco: una carpeta por grabación con `session.json`.

use crate::types::{win_path, Recording, TrackFile, TrackKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod storage;
mod mp3;
pub use mp3::encode_mp3;
pub(crate) use mp3::StreamingMp3;
pub use storage::{export_single, reserve_direct_mp3, finish_direct_mp3};

pub const FILE_INPUT: &str = "Microfono.wav";
pub const FILE_OUTPUT: &str = "Sistema.wav";
pub const FILE_MIX: &str = "Mezcla.wav";
pub const SESSION_JSON: &str = "session.json";
static CATALOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionJson {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub duration_ms: u64,
}

pub struct NewSession {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub dir: PathBuf,
}

/// Crea `<outputDir>\Eco AAAA-MM-DD HH-MM-SS\` (con sufijo si ya existe).
pub fn create_session_dir(output_dir: &str) -> Result<NewSession, String> {
    let now = chrono::Local::now();
    let root = PathBuf::from(output_dir);
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("No se pudo crear la carpeta de grabaciones «{output_dir}»: {e}"))?;
    let stamp = now.format("%Y-%m-%d %H-%M-%S").to_string();
    let id_base = now.format("%Y%m%d-%H%M%S-%f").to_string();
    for n in 1..100 {
        let (folder, id) = if n == 1 {
            (format!("Eco {stamp}"), id_base.clone())
        } else {
            (format!("Eco {stamp} ({n})"), format!("{id_base}-{n}"))
        };
        let dir = root.join(folder);
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                return Ok(NewSession {
                    id,
                    name: now.format("Запись %d/%m %H:%M").to_string(),
                    created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
                    dir,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("No se pudo crear la carpeta de la sesión: {e}")),
        }
    }
    Err("No se pudo crear una carpeta de sesión única".into())
}

pub fn write_session_json(dir: &Path, s: &SessionJson) -> Result<(), String> {
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(SESSION_JSON), json)
        .map_err(|e| format!("No se pudo guardar session.json: {e}"))
}

fn read_session_json(dir: &Path) -> Option<SessionJson> {
    let t = std::fs::read_to_string(dir.join(SESSION_JSON)).ok()?;
    serde_json::from_str(&t).ok()
}

pub fn track_file(kind: TrackKind, path: &Path) -> Option<TrackFile> {
    let size = std::fs::metadata(path).ok()?.len();
    let spec = hound::WavReader::open(path).ok()?.spec();
    Some(TrackFile {
        kind,
        path: win_path(&path.to_string_lossy()),
        size_bytes: size,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}

/// Construye el `Recording` de una carpeta de sesión a partir de lo que hay en disco.
pub fn recording_from_dir(dir: &Path) -> Option<Recording> {
    let sj = read_session_json(dir)?;
    let mut files = Vec::new();
    for (name, kind) in [
        (FILE_INPUT, TrackKind::Input),
        (FILE_OUTPUT, TrackKind::Output),
        (FILE_MIX, TrackKind::Mix),
    ] {
        if let Some(f) = track_file(kind, &dir.join(name)).or_else(|| storage::mp3_track(kind, &dir.join(name).with_extension("mp3"))) {
            files.push(f);
        }
    }
    Some(Recording {
        id: sj.id,
        name: sj.name,
        created_at: sj.created_at,
        duration_ms: sj.duration_ms,
        files,
        folder: win_path(&dir.to_string_lossy()),
    })
}

/// Escanea las subcarpetas con `session.json`, de más nueva a más vieja.
pub fn list_recordings(output_dir: &str) -> Vec<Recording> {
    let _guard = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<Recording> = match std::fs::read_dir(output_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter_map(|p| recording_from_dir(&p))
            .collect(),
        Err(_) => Vec::new(),
    };
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    out.extend(storage::list_flat(Path::new(output_dir)));
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    out
}

fn find_dir(output_dir: &str, id: &str) -> Result<PathBuf, String> {
    let rd = std::fs::read_dir(output_dir)
        .map_err(|_| "No se encontró la carpeta de grabaciones.".to_string())?;
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() && read_session_json(&p).map(|s| s.id == id).unwrap_or(false) {
            return Ok(p);
        }
    }
    Err("No se encontró la grabación.".into())
}

pub fn rename_recording(output_dir: &str, id: &str, name: &str) -> Result<Recording, String> {
    let _guard = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let name = name.trim();
    if name.is_empty() {
        return Err("El nombre no puede estar vacío.".into());
    }
    if id.starts_with("flat:") {
        return storage::rename_flat(Path::new(output_dir), id, name);
    }
    let dir = find_dir(output_dir, id)?;
    storage::check_child(Path::new(output_dir), &dir)?;
    let mut sj = read_session_json(&dir).ok_or("No se pudo leer la sesión.")?;
    sj.name = name.to_string();
    write_session_json(&dir, &sj)?;
    recording_from_dir(&dir).ok_or_else(|| "No se pudo leer la sesión.".into())
}

/// Manda la carpeta a la Papelera; si no es posible, la borra.
pub fn delete_recording(output_dir: &str, id: &str) -> Result<(), String> {
    let _guard = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if id.starts_with("flat:") {
        return storage::delete_flat(Path::new(output_dir), id);
    }
    let dir = find_dir(output_dir, id)?;
    // Never recursively delete a session: it may contain unrelated user files or a junction.
    storage::check_child(Path::new(output_dir), &dir)?;
    for name in [FILE_INPUT, FILE_OUTPUT, FILE_MIX] {
        for path in [dir.join(name), dir.join(name).with_extension("mp3")] {
            if path.is_file() {
                storage::check_child(&dir, &path)?;
                storage::remove_audio_file(&path)?;
            }
        }
    }
    std::fs::remove_file(dir.join(SESSION_JSON)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir(&dir); // non-empty folders are deliberately retained
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_rename_sort() {
        let root = std::env::temp_dir().join("eco-library-test");
        let _ = std::fs::remove_dir_all(&root);
        let r = root.to_string_lossy().to_string();
        let a = create_session_dir(&r).unwrap();
        let b = create_session_dir(&r).unwrap();
        assert_ne!(a.dir, b.dir);
        for (s, created) in [(&a, "2026-10-03T10:00:00-04:00"), (&b, "2026-10-03T11:00:00-04:00")] {
            write_session_json(
                &s.dir,
                &SessionJson {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    created_at: created.into(),
                    duration_ms: 1000,
                },
            )
            .unwrap();
        }
        let l = list_recordings(&r);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].id, b.id); // más nueva primero
        let rr = rename_recording(&r, &a.id, "  Prueba ").unwrap();
        assert_eq!(rr.name, "Prueba");
        assert!(rename_recording(&r, &a.id, " ").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn folder_and_track_paths_use_windows_separators() {
        let root = std::env::temp_dir().join("eco-library-test-sep");
        let _ = std::fs::remove_dir_all(&root);
        let fwd = root.to_string_lossy().replace('\u{5c}', "/"); // outputDir con «/»
        let s = create_session_dir(&fwd).unwrap();
        write_session_json(&s.dir, &SessionJson { id: s.id.clone(), name: s.name.clone(), created_at: "2026-10-03T10:00:00-04:00".into(), duration_ms: 1 }).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(s.dir.join(FILE_INPUT), spec).unwrap();
        w.write_sample(0i16).unwrap();
        w.finalize().unwrap();
        let l = list_recordings(&fwd);
        assert_eq!(l.len(), 1);
        assert!(!l[0].folder.contains('/'), "{}", l[0].folder);
        assert_eq!(l[0].files.len(), 1);
        assert!(!l[0].files[0].path.contains('/'), "{}", l[0].files[0].path);
        let _ = std::fs::remove_dir_all(&root);
    }
}
