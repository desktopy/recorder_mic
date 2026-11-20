//! Sonda de consola: usa el MISMO motor que la app (`eco_audio::Engine`), sin ventanas.
//! Lista dispositivos, graba 4 s (micrófono + loopback, modo «both») y hace una prueba con pausa.
//! Salida: <directorio temporal del sistema>/eco-probe/ (o la ruta de `ECO_PROBE_OUT`).

use eco_audio::audio::{devices, wav};
use eco_audio::types::*;
use eco_audio::{Engine, EngineEvent, EventSink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Counters {
    levels: AtomicUsize,
    states: AtomicUsize,
    errors: Mutex<Vec<String>>,
    max_peak: Mutex<[f32; 2]>,
}

fn print_devices(d: &Devices) {
    for (title, list) in [("ENTRADAS", &d.inputs), ("SALIDAS", &d.outputs)] {
        println!("{title}:");
        for x in list {
            println!(
                "  [{}] id={:?} name={:?} {} Hz {} ch",
                if x.is_default { "*" } else { " " },
                x.id,
                x.name,
                x.sample_rate,
                x.channels
            );
        }
    }
}

/// Imprime una pista y devuelve su duración en segundos.
fn report(label: &str, path: &Path) -> Option<f64> {
    match wav::analyze(path) {
        Ok((sr, ch, dur, peak)) => {
            println!("  {label:<9} {}  sr={sr} ch={ch} dur={dur:.3}s peak={peak:.5}", path.display());
            Some(dur)
        }
        Err(e) => {
            println!("  {label:<9} {} -> FALTA/ERROR: {e}", path.display());
            None
        }
    }
}

fn check_recording(rec: &Recording) -> bool {
    let mut ok = true;
    println!("Grabación {} «{}» durationMs={} carpeta={}", rec.id, rec.name, rec.duration_ms, rec.folder);
    for (label, file) in [("Microfono", "Microfono.wav"), ("Sistema", "Sistema.wav"), ("Mezcla", "Mezcla.wav")] {
        let p = PathBuf::from(&rec.folder).join(file);
        match report(label, &p) {
            Some(d) if (3.8..=4.2).contains(&d) => {}
            Some(d) => {
                println!("    FAIL: {label} dura {d:.3}s (esperado 3.8–4.2)");
                ok = false;
            }
            None => ok = false,
        }
    }
    ok
}

fn main() {
    let out = std::env::var_os("ECO_PROBE_OUT").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("eco-probe"));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let devs = devices::list_devices();
    print_devices(&devs);

    let counters = Arc::new(Counters::default());
    let sink: EventSink = {
        let c = counters.clone();
        Arc::new(move |ev| match ev {
            EngineEvent::Levels(l) => {
                c.levels.fetch_add(1, Relaxed);
                let mut m = c.max_peak.lock().unwrap();
                m[0] = m[0].max(l.input.peak);
                m[1] = m[1].max(l.output.peak);
            }
            EngineEvent::State(_) => {
                c.states.fetch_add(1, Relaxed);
            }
            EngineEvent::Error(e) => c.errors.lock().unwrap().push(format!("{e:?}")),
            EngineEvent::RecordingSaved(r) => println!("  [evento] recording-saved {}", r.id),
            EngineEvent::DevicesChanged(_) => println!("  [evento] devices-changed"),
        })
    };

    let settings = Settings {
        output_dir: out.to_string_lossy().into_owned(),
        mode: Mode::Both,
        format: Format::Wav24,
        monitor_when_idle: false,
        ..Settings::default()
    };
    let eng = Engine::new(settings, sink, false); // persist=false: no toca %APPDATA%
    let mut all_ok = true;

    println!("\n=== Prueba 1: 4 s continuos (modo both) ===");
    let st = eng.start_recording().expect("start_recording");
    println!("start -> {:?}", st.status);
    for (role, i) in ["micrófono", "loopback"].iter().zip(eng.stream_info()) {
        println!("  stream {role}: {i:?}");
    }
    let n0 = counters.levels.load(Relaxed);
    std::thread::sleep(Duration::from_secs(4));
    let rate = (counters.levels.load(Relaxed) - n0) as f64 / 4.0;
    println!("  levels: {rate:.1} eventos/s durante los 4 s");
    all_ok &= (25.0..=36.0).contains(&rate);
    println!("  estado a 4 s: {:?}", eng.state());
    let rec = eng.stop_recording().expect("stop_recording");
    all_ok &= check_recording(&rec);

    println!("\n=== Prueba 2: 2 s grabando, 1 s pausa, 2 s grabando ===");
    eng.start_recording().expect("start 2");
    std::thread::sleep(Duration::from_secs(2));
    let st = eng.pause_recording().expect("pause");
    println!("pause -> {:?} elapsedMs={}", st.status, st.elapsed_ms);
    std::thread::sleep(Duration::from_secs(1));
    let st = eng.resume_recording().expect("resume");
    println!("resume -> {:?} elapsedMs={}", st.status, st.elapsed_ms);
    std::thread::sleep(Duration::from_secs(2));
    let rec2 = eng.stop_recording().expect("stop 2");
    all_ok &= check_recording(&rec2);

    println!("\n=== Prueba 3: biblioteca ===");
    let list = eng.list_recordings();
    let newest_first = list.first().map(|r| r.id == rec2.id).unwrap_or(false);
    println!("list_recordings -> {} (más nueva primero: {newest_first})", list.len());
    all_ok &= list.len() == 2 && newest_first;
    drop(list);

    println!("\n=== Prueba 4: desconexión simulada del loopback a los 1.5 s ===");
    eng.start_recording().expect("start 4");
    std::thread::sleep(Duration::from_millis(1500));
    eng.simulate_device_loss(false);
    let t0 = std::time::Instant::now();
    while eng.state().status != Status::Idle && t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    let st = eng.state();
    println!("  estado tras la pérdida: {:?}", st.status);
    all_ok &= st.status == Status::Idle;
    let rec4 = eng.list_recordings().into_iter().next().unwrap();
    println!("  guardada: {} durationMs={}", rec4.id, rec4.duration_ms);
    for f in &rec4.files {
        let d = report(&format!("{:?}", f.kind), Path::new(&f.path));
        all_ok &= d.map(|d| (1.3..=1.8).contains(&d)).unwrap_or(false);
    }
    let fatal: Vec<String> = counters.errors.lock().unwrap().iter().filter(|e| e.contains("fatal: true")).cloned().collect();
    println!("  engine-error fatales: {fatal:?}");
    all_ok &= fatal.len() == 1;
    // el motor debe poder grabar otra vez (la fuente caída se reabre sola)
    eng.start_recording().expect("start tras pérdida");
    std::thread::sleep(Duration::from_millis(500));
    let rec_again = eng.stop_recording().expect("stop tras pérdida");
    println!("  grabación posterior OK: {} ({} archivos)", rec_again.id, rec_again.files.len());

    println!("\n=== Prueba 5: modo mixed + wav16 (solo debe quedar Mezcla.wav) ===");
    let mut s = eng.settings();
    s.mode = Mode::Mixed;
    s.format = Format::Wav16;
    eng.set_settings(s).expect("set_settings");
    eng.start_recording().expect("start 5");
    std::thread::sleep(Duration::from_millis(1500));
    let rec5 = eng.stop_recording().expect("stop 5");
    for f in &rec5.files {
        report(&format!("{:?}", f.kind), Path::new(&f.path));
    }
    all_ok &= rec5.files.len() == 1 && rec5.files[0].kind == TrackKind::Mix;
    println!("  archivos en la carpeta: {:?}", std::fs::read_dir(&rec5.folder).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>());

    println!("\n=== Prueba 6: monitorWhenIdle=true → niveles sin grabar ===");
    let mut s = eng.settings();
    s.monitor_when_idle = true;
    eng.set_settings(s).expect("set_settings 6");
    std::thread::sleep(Duration::from_millis(500));
    let n0 = counters.levels.load(Relaxed);
    std::thread::sleep(Duration::from_secs(1));
    let idle_rate = (counters.levels.load(Relaxed) - n0) as f64;
    println!("  levels en reposo: {idle_rate:.0} eventos en 1 s");
    all_ok &= idle_rate >= 20.0;

    let n = counters.levels.load(Relaxed);
    println!("\nEventos: levels={n} (≈{:.1}/s en ~8 s grabando), state={}", n as f64 / 8.0, counters.states.load(Relaxed));
    println!("Pico máx. en medidores [mic, sistema]: {:?}", *counters.max_peak.lock().unwrap());
    println!("engine-error recibidos: {:?}", counters.errors.lock().unwrap());
    eng.shutdown();
    println!("\nRESULTADO: {}", if all_ok { "PASS" } else { "FAIL" });
    if !all_ok {
        std::process::exit(1);
    }
}
