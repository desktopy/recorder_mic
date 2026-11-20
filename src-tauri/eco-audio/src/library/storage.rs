//! Single-file recordings use one shared index, never a session folder as deletion target.
use super::*;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};

const INDEX: &str = ".eco-library.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct Catalog { root: String, records: Vec<Recording> }

fn index_path(root: &Path) -> Result<PathBuf, String> {
    let key = root.canonicalize().map_err(|e| e.to_string())?.to_string_lossy().to_lowercase();
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hash);
    #[cfg(not(test))]
    let base = PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA не определён; индекс не создан")?).join("Eco").join("library-v6");
    // Test metadata is outside the Desktop-like output folder and never in real AppData.
    #[cfg(test)]
    let base = root.with_extension("eco-metadata");
    Ok(base.join(format!("{:016x}.json", hash.finish())))
}

fn root_key(root: &Path) -> Result<String, String> {
    Ok(root.canonicalize().map_err(|e| e.to_string())?.to_string_lossy().to_lowercase())
}

pub fn mp3_track(kind: TrackKind, path: &Path) -> Option<TrackFile> {
    let size = fs::metadata(path).ok()?.len();
    let mut bytes = vec![0u8; 65536];
    let n = fs::File::open(path).ok()?.read(&mut bytes).ok()?;
    bytes.truncate(n);
    let offset = if bytes.starts_with(b"ID3") && bytes.len() >= 10 {
        10 + bytes[6..10].iter().fold(0usize, |n, b| (n << 7) | (b & 0x7f) as usize)
    } else { 0 };
    let header = bytes.get(offset..)?.windows(4).find(|h| h[0] == 0xff && h[1] & 0xe0 == 0xe0
        && h[1] & 6 == 2 && (h[1] >> 3) & 3 != 1 && h[2] >> 4 != 0
        && h[2] >> 4 != 15 && (h[2] >> 2) & 3 != 3)?;
    let divisor = match (header[1] >> 3) & 3 { 3 => 1, 2 => 2, _ => 4 };
    let sample_rate = [44100, 48000, 32000][((header[2] >> 2) & 3) as usize] / divisor;
    let channels = if header[3] >> 6 == 3 { 1 } else { 2 };
    Some(TrackFile { kind, path: win_path(&path.to_string_lossy()), size_bytes: size,
        sample_rate, channels })
}

fn read_index(root: &Path) -> Result<Vec<Recording>, String> {
    match fs::read_to_string(index_path(root)?) {
        Ok(t) => {
            let catalog: Catalog = serde_json::from_str(&t).map_err(|e| format!("Индекс повреждён; он не перезаписан: {e}"))?;
            if catalog.root != root_key(root)? { return Err("Конфликт каталога метаданных; запись отменена".into()); }
            return Ok(catalog.records);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(e.to_string()),
    }
    // Import legacy flat recordings on first write. Never alter the old Desktop index.
    match fs::read_to_string(root.join(INDEX)) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("Индекс записей повреждён; он не перезаписан: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

fn write_index(root: &Path, records: &[Recording]) -> Result<(), String> {
    let path = index_path(root)?;
    fs::create_dir_all(path.parent().ok_or("Нет каталога метаданных")?).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("{}-{}.tmp", std::process::id(), chrono::Local::now().timestamp_nanos_opt().unwrap_or_default()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e| e.to_string())?;
    let result = (|| {
        let catalog = Catalog { root: root_key(root)?, records: records.to_vec() };
        file.write_all(serde_json::to_string_pretty(&catalog).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() { let _ = fs::remove_file(tmp); }
    result
}

pub fn check_child(root: &Path, path: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 { return Err("Ссылки и junction не изменяются.".into()); }
    }
    if meta.file_type().is_symlink() || path.canonicalize().map_err(|e| e.to_string())?.parent() != Some(root.as_path()) {
        return Err("Небезопасный путь записи: операция отменена.".into());
    }
    Ok(())
}

pub fn list_flat(root: &Path) -> Vec<Recording> {
    read_index(root).unwrap_or_default().into_iter().filter(|r|
        r.files.len() == 1 && check_child(root, Path::new(&r.files[0].path)).is_ok()).map(|mut r| {
            if r.duration_ms == 0 && r.files[0].size_bytes == 0 {
                r.name.push_str(" (не завершена; проверьте MP3)");
                r.files[0].size_bytes = fs::metadata(&r.files[0].path).map(|m|m.len()).unwrap_or(0);
            }
            r
        }).collect()
}

fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.ends_with(['.', ' ']) && !name.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && !["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"].contains(&name.split('.').next().unwrap_or("").to_uppercase().as_str())
}

/// Reserve exactly one audio file and persist its identity before starting capture.
/// The pending entry allows a partial MP3 to remain discoverable after interruption;
/// it does not assert that an interrupted stream is complete or decodable.
pub fn reserve_direct_mp3(root: &Path, kind: TrackKind, sample_rate: u32, channels: u16) -> Result<(Recording, fs::File), String> {
    let _guard = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut records = read_index(root)?;
    let now = chrono::Local::now();
    let prefix = if kind == TrackKind::Input { "recmic" } else { "recsys" };
    let base = format!("{prefix}_{}", now.format("%Y-%m-%d_%H-%M"));
    for n in 1..10000 {
        let stem = if n == 1 { base.clone() } else { format!("{base}_{n}") };
        let path = root.join(format!("{stem}.mp3"));
        if path.with_extension("wav").exists() { continue; }
        let output = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        };
        let rec = Recording {
            id: format!("flat:{}-{n}", now.format("%Y%m%d-%H%M%S-%f")), name: stem,
            created_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false), duration_ms: 0,
            folder: win_path(&root.to_string_lossy()),
            files: vec![TrackFile { kind, path: win_path(&path.to_string_lossy()), size_bytes: 0,
                sample_rate: if [8000,11025,12000,16000,22050,24000,32000,44100,48000].contains(&sample_rate) {sample_rate} else {48000}, channels: channels.min(2) }],
        };
        records.push(rec.clone());
        if let Err(e) = write_index(root, &records) {
            drop(output);
            let _ = fs::remove_file(&path); // only our still-empty reserved file
            return Err(e);
        }
        return Ok((rec, output));
    }
    Err("Нет свободного имени записи".into())
}

pub fn finish_direct_mp3(root: &Path, id: &str, path: &Path, kind: TrackKind, duration_ms: u64) -> Result<Recording, String> {
    let _guard = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    check_child(root, path)?;
    let mut records = read_index(root)?;
    let rec = records.iter_mut().find(|r| r.id == id).ok_or("Метаданные записи не найдены; MP3 оставлен на диске")?;
    if rec.files.len() != 1 || Path::new(&rec.files[0].path) != path { return Err("Несовпадение пути записи".into()); }
    rec.duration_ms = duration_ms;
    rec.files[0] = mp3_track(kind, path).ok_or_else(|| format!("MP3 не имеет читаемого заголовка; полученный файл оставлен: {}", path.display()))?;
    let result = rec.clone();
    write_index(root, &records)?;
    Ok(result)
}

fn copy_new(source: &Path, dest: &Path) -> Result<(), std::io::Error> {
    let mut output = OpenOptions::new().write(true).create_new(true).open(dest)?;
    let result = (|| {
        std::io::copy(&mut fs::File::open(source)?, &mut output)?;
        output.sync_all()
    })();
    drop(output);
    if result.is_err() { let _ = fs::remove_file(dest); }
    result
}

pub fn export_single(dir: &Path, sj: &SessionJson, track: &TrackFile) -> Result<Recording, String> {
    let _guard = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = dir.parent().ok_or("Нет папки сохранения")?;
    let mut records = read_index(root)?;
    let created = chrono::DateTime::parse_from_rfc3339(&sj.created_at).map_err(|e| e.to_string())?;
    let prefix = if track.kind == TrackKind::Input { "recmic" } else { "recsys" };
    let base = format!("{prefix}_{}", created.format("%Y-%m-%d_%H-%M"));
    let source = Path::new(&track.path);
    let ext = source.extension().and_then(|s| s.to_str()).ok_or("Нет формата")?;
    for n in 1..10000 {
        let stem = if n == 1 { base.clone() } else { format!("{base}_{n}") };
        let dest = root.join(format!("{stem}.{ext}"));
        if dest.with_extension("wav").exists() || dest.with_extension("mp3").exists() { continue; }
        match copy_new(source, &dest) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
            Ok(()) => {}
        }
        let mut file = track.clone();
        file.path = win_path(&dest.to_string_lossy());
        let rec = Recording { id: format!("flat:{}", sj.id), name: stem, created_at: sj.created_at.clone(),
            duration_ms: sj.duration_ms, files: vec![file], folder: win_path(&root.to_string_lossy()) };
        records.push(rec.clone());
        if let Err(e) = write_index(root, &records) {
            return Err(format!("{e}. Копия сохранена: {}", dest.display()));
        }
        for name in [FILE_INPUT, FILE_OUTPUT, FILE_MIX] {
            let _ = fs::remove_file(dir.join(name));
            let _ = fs::remove_file(dir.join(name).with_extension("mp3"));
        }
        let _ = fs::remove_file(dir.join(SESSION_JSON));
        let _ = fs::remove_dir(dir);
        return Ok(rec);
    }
    Err("Не удалось подобрать свободное имя записи; исходная запись сохранена.".into())
}

pub fn rename_flat(root: &Path, id: &str, name: &str) -> Result<Recording, String> {
    if !safe_name(name) { return Err("Недопустимое имя файла.".into()); }
    let mut records = read_index(root)?;
    let rec = records.iter_mut().find(|r| r.id == id).ok_or("Запись не найдена")?;
    if rec.files.len() != 1 { return Err("Некорректная запись".into()); }
    let old = PathBuf::from(&rec.files[0].path);
    check_child(root, &old)?;
    let ext = old.extension().and_then(|s| s.to_str()).ok_or("Нет формата")?;
    let dest = root.join(format!("{name}.{ext}"));
    if dest == old { return Ok(rec.clone()); }
    copy_new(&old, &dest).map_err(|e| format!("Файл не переименован (возможно, имя занято): {e}"))?;
    rec.name = name.into();
    rec.files[0].path = win_path(&dest.to_string_lossy());
    let result = rec.clone();
    write_index(root, &records)?;
    fs::remove_file(old).map_err(|e| format!("Новое имя сохранено, старый файл не удалён: {e}"))?;
    Ok(result)
}

pub fn remove_audio_file(path: &Path) -> Result<(), String> {
    // Recycle individual audio files, never their parent. Unit tests must not touch the user's bin.
    #[cfg(not(test))]
    if trash::delete(path).is_ok() && !path.exists() { return Ok(()); }
    fs::remove_file(path).map_err(|e| e.to_string())
}

pub fn delete_flat(root: &Path, id: &str) -> Result<(), String> {
    let mut records = read_index(root)?;
    let rec = records.iter().find(|r| r.id == id).ok_or("Запись не найдена")?;
    if rec.files.len() != 1 { return Err("Некорректная запись".into()); }
    let path = Path::new(&rec.files[0].path);
    check_child(root, path)?;
    if !matches!(path.extension().and_then(|s|s.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("wav" | "mp3")) {
        return Err("Удалять можно только WAV/MP3 записи.".into());
    }
    if !path.is_file() { return Err("Удалять можно только аудиофайл.".into()); }
    remove_audio_file(path)?;
    records.retain(|r| r.id != id);
    write_index(root, &records)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("eco-storage-{}-{}", std::process::id(), chrono::Local::now().timestamp_nanos_opt().unwrap()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for TempRoot { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); let _ = fs::remove_dir_all(self.0.with_extension("eco-metadata")); } }
    fn session(root: &Path) -> (NewSession, SessionJson) {
        let s = create_session_dir(root.to_str().unwrap()).unwrap();
        let sj = SessionJson { id: s.id.clone(), name: s.name.clone(), created_at: s.created_at.clone(), duration_ms: 1000 };
        write_session_json(&s.dir, &sj).unwrap();
        let mut w = crate::audio::wav::TrackWriter::create(&s.dir.join(FILE_INPUT), 48000, 1, crate::types::Format::Wav24).unwrap();
        let samples: Vec<f32> = (0..48000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin() * 0.1).collect();
        w.write(&samples).unwrap();
        w.finalize().unwrap();
        (s, sj)
    }

    fn independently_decode(mp3: &Path, decoded: &Path) {
        let decoder = PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("ffmpeg.exe");
        let mut cmd = std::process::Command::new(decoder);
        #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
        let mut child = cmd.args(["-nostdin", "-v", "error", "-xerror", "-err_detect", "explode", "-i"]).arg(mp3)
            .args(["-c:a", "pcm_f32le"]).arg(decoded).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().unwrap() { assert!(status.success(), "independent MP3 decode failed: {}", mp3.display()); break; }
            if std::time::Instant::now() >= deadline { let _ = child.kill(); let _ = child.wait(); panic!("decoder timeout"); }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let reader = hound::WavReader::open(decoded).unwrap();
        assert!(reader.duration() > reader.spec().sample_rate / 4);
        let data: Vec<f32> = reader.into_samples::<f32>().map(Result::unwrap).collect();
        assert!(data.iter().all(|s| s.is_finite()));
        assert!(data.iter().map(|s|s*s).sum::<f32>() / data.len() as f32 > 0.0001);
    }

    #[test]
    fn direct_stream_grows_decodes_before_and_after_finish_and_desktop_stays_clean() {
        let root = TempRoot::new();
        for (rate, channels) in [(48000,1), (44100,2), (96000,1), (48000,6)] {
            let (rec, file) = reserve_direct_mp3(&root.0, TrackKind::Input, rate, channels).unwrap();
            let path = PathBuf::from(&rec.files[0].path);
            let mut writer = crate::audio::wav::TrackWriter::direct_mp3(file, rate, channels).unwrap();
            let mut previous = 0;
            for second in 0..3 {
                let pcm: Vec<f32> = (0..rate).flat_map(|i| (0..channels).map(move |_| (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 0.1)).collect();
                writer.write(&pcm).unwrap(); writer.flush().unwrap();
                let size = fs::metadata(&path).unwrap().len();
                assert!(size > previous, "stream did not grow at {second}"); previous = size;
            }
            // Decode the actual live file, not a WAV-derived MP3 or an in-memory copy.
            let decoded = root.0.with_extension("eco-metadata").join(format!("live-{rate}-{channels}.wav"));
            independently_decode(&path, &decoded);
            assert_eq!(writer.finalize().unwrap(), rate as u64 * 3);
            let decoded = decoded.with_file_name(format!("final-{rate}-{channels}.wav"));
            independently_decode(&path, &decoded);
            let recording = finish_direct_mp3(&root.0, &rec.id, &path, TrackKind::Input, 3000).unwrap();
            assert_eq!(recording.duration_ms, 3000);
            assert!(!rec.name.contains(':'));
            assert_eq!(rec.name.split('_').nth(1).unwrap().len(), 10);
            assert_eq!(rec.name.split('_').nth(2).unwrap().len(), 5); // no seconds
        }
        let items: Vec<_> = fs::read_dir(&root.0).unwrap().map(|e|e.unwrap().path()).collect();
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|p|p.extension().unwrap() == "mp3"));
        assert_eq!(list_recordings(root.0.to_str().unwrap()).len(), 4);
        // A dropped encoder leaves already-flushed audio, not an assertion of recovery.
        let (rec, file) = reserve_direct_mp3(&root.0, TrackKind::Input, 48000, 1).unwrap();
        let mut writer = crate::audio::wav::TrackWriter::direct_mp3(file, 48000, 1).unwrap();
        writer.write(&(0..48000).map(|i|(i as f32 * 0.05).sin()*0.1).collect::<Vec<_>>()).unwrap();
        writer.flush().unwrap();
        assert!(writer.write(&[f32::NAN]).is_err());
        drop(writer);
        independently_decode(Path::new(&rec.files[0].path), &root.0.with_extension("eco-metadata").join("interrupted.wav"));
        assert_eq!(list_recordings(root.0.to_str().unwrap()).len(), 5);
    }

    #[test]
    fn direct_collisions_legacy_migration_and_delete_are_safe() {
        let root = TempRoot::new();
        let (legacy, sj) = session(&root.0);
        let old = export_single(&legacy.dir, &sj, &track_file(TrackKind::Input, &legacy.dir.join(FILE_INPUT)).unwrap()).unwrap();
        // Simulate a pre-v6 shared Desktop index, preserving it byte-for-byte.
        fs::remove_file(index_path(&root.0).unwrap()).unwrap();
        let legacy_json = serde_json::to_string(&vec![old.clone()]).unwrap();
        fs::write(root.0.join(INDEX), &legacy_json).unwrap();
        let (a, file) = reserve_direct_mp3(&root.0, TrackKind::Input, 48000, 1).unwrap(); drop(file);
        let (b, file) = reserve_direct_mp3(&root.0, TrackKind::Input, 48000, 1).unwrap(); drop(file);
        assert_ne!(a.files[0].path, b.files[0].path);
        assert_ne!(a.name, old.name); // preexisting WAV reserves the same stem
        assert_eq!(fs::read_to_string(root.0.join(INDEX)).unwrap(), legacy_json);
        assert_eq!(list_flat(&root.0).len(), 3);
        assert!(rename_recording(root.0.to_str().unwrap(), &a.id, "COM9").is_err());
        assert!(rename_recording(root.0.to_str().unwrap(), &a.id, "../escape").is_err());
        fs::write(root.0.join("keep.txt"), "keep").unwrap();
        delete_recording(root.0.to_str().unwrap(), &a.id).unwrap();
        delete_recording(root.0.to_str().unwrap(), &old.id).unwrap();
        assert_eq!(list_flat(&root.0).len(), 1); // legacy entry must not resurrect
        assert!(Path::new(&b.files[0].path).exists());
        assert_eq!(fs::read_to_string(root.0.join("keep.txt")).unwrap(), "keep");
        assert!(root.0.is_dir());
    }
    #[test]
    fn single_file_collision_rename_delete_and_legacy_safety() {
        let root = TempRoot::new();
        let r = root.0.to_str().unwrap();
        fs::write(root.0.join("unrelated.txt"), "keep").unwrap();
        let (a, sj) = session(&root.0);
        let track = track_file(TrackKind::Input, &a.dir.join(FILE_INPUT)).unwrap();
        let rec = export_single(&a.dir, &sj, &track).unwrap();
        assert!(!a.dir.exists());
        assert!(rec.name.starts_with("recmic_"));
        assert!(Path::new(&rec.files[0].path).is_file());
        let (b, mut sj2) = session(&root.0);
        fs::write(b.dir.join("session.mp3"), "unrelated file").unwrap();
        sj2.created_at = sj.created_at.clone();
        let rec2 = export_single(&b.dir, &sj2, &track_file(TrackKind::Input, &b.dir.join(FILE_INPUT)).unwrap()).unwrap();
        assert_eq!(fs::read_to_string(b.dir.join("session.mp3")).unwrap(), "unrelated file");
        assert_eq!(rec2.name, format!("{}_2", rec.name));
        assert_ne!(rec.id, rec2.id);
        assert!(rename_recording(r, &rec.id, &rec2.name).is_err());
        assert!(rename_recording(r, &rec.id, "../escape").is_err());
        let renamed = rename_recording(r, &rec.id, "Проверка").unwrap();
        assert!(Path::new(&renamed.files[0].path).is_file());
        assert!(!Path::new(&rec.files[0].path).exists());
        let (legacy, _) = session(&root.0);
        fs::write(legacy.dir.join("user.txt"), "keep").unwrap();
        assert_eq!(list_recordings(r).len(), 3);
        delete_recording(r, &renamed.id).unwrap();
        assert!(root.0.join("unrelated.txt").is_file());
        assert!(Path::new(&rec2.files[0].path).is_file());
        delete_recording(r, &legacy.id).unwrap();
        assert!(legacy.dir.join("user.txt").is_file());
        assert!(root.0.is_dir());
    }
    #[test]
    fn mp3_success_and_encoding_failure_preserve_wav() {
        let root = TempRoot::new();
        let (s, sj) = session(&root.0);
        let wav = s.dir.join(FILE_INPUT);
        let mp3 = encode_mp3(&wav).unwrap();
        assert!(wav.is_file());
        let track = mp3_track(TrackKind::Input, &mp3).unwrap();
        assert_eq!((track.sample_rate, track.channels), (48000, 1), "MP3 prefix: {:02x?}", &fs::read(&mp3).unwrap()[..64]);
        // Existing output must not be overwritten and the input must survive.
        let before = fs::read(&mp3).unwrap();
        assert!(encode_mp3(&wav).is_err());
        assert_eq!(fs::read(&mp3).unwrap(), before);
        assert!(wav.is_file());
        let rec = export_single(&s.dir, &sj, &track).unwrap();
        assert!(rec.files[0].path.ends_with(".mp3"));
        let (bad, _) = session(&root.0);
        let invalid = bad.dir.join("invalid.wav");
        fs::write(&invalid, "not a WAV").unwrap();
        assert!(encode_mp3(&invalid).is_err());
        assert_eq!(fs::read_to_string(invalid).unwrap(), "not a WAV");
        let empty = bad.dir.join("empty.wav");
        crate::audio::wav::TrackWriter::create(&empty, 48000, 1, crate::types::Format::Wav24).unwrap().finalize().unwrap();
        assert!(encode_mp3(&empty).is_err());
        assert!(hound::WavReader::open(empty).is_ok());
    }
    #[test]
    fn forged_root_path_and_corrupt_catalog_are_not_deleted_or_replaced() {
        let root = TempRoot::new();
        let (s, sj) = session(&root.0);
        let rec = export_single(&s.dir, &sj, &track_file(TrackKind::Input, &s.dir.join(FILE_INPUT)).unwrap()).unwrap();
        let mut forged = rec.clone();
        forged.files[0].path = root.0.to_string_lossy().into_owned();
        write_index(&root.0, &[forged]).unwrap();
        assert!(delete_recording(root.0.to_str().unwrap(), &rec.id).is_err());
        assert!(root.0.is_dir());
        fs::write(index_path(&root.0).unwrap(), "broken index").unwrap();
        let (b, sj2) = session(&root.0);
        assert!(export_single(&b.dir, &sj2, &track_file(TrackKind::Input, &b.dir.join(FILE_INPUT)).unwrap()).is_err());
        assert!(b.dir.join(FILE_INPUT).is_file());
        assert_eq!(fs::read_to_string(index_path(&root.0).unwrap()).unwrap(), "broken index");
    }
}
