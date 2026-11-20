//! Sonda del micrófono: graba N segundos del micrófono real con «Voz automática» activada y desactivada
//! y compara pico/RMS de `Microfono.wav`. Solo LEE el volumen del micrófono en Windows (no lo cambia).
//!
//! Uso: `cargo run --release --example mic_probe -- [segundos=4] [gananciaDb=0] [speak]`
//! Con `speak` se reproduce la misma frase con la voz sintética de Windows por los altavoces durante cada
//! grabación (no cambia el volumen del sistema), para comparar AGC activado/desactivado con la misma señal.

use eco_audio::audio::endpoint;
use eco_audio::audio::wav::TrackReader;
use eco_audio::types::*;
use eco_audio::Engine;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// (pico, rms global, rms de los bloques de 100 ms más fuertes (percentil 90)) en dBFS.
fn stats(path: &Path) -> (f32, f32, f32) {
    let mut r = TrackReader::open(path).expect("abrir WAV");
    let ch = r.spec.channels.max(1) as usize;
    let mut all = Vec::new();
    let mut buf = Vec::new();
    loop {
        r.read_samples(4096 * ch, &mut buf);
        if buf.is_empty() {
            break;
        }
        all.extend_from_slice(&buf);
    }
    let peak = all.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt();
    let win = (r.spec.sample_rate as usize / 10) * ch;
    let mut blocks: Vec<f32> = all.chunks(win).filter(|c| c.len() == win).map(rms).collect();
    blocks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p90 = blocks.get(blocks.len() * 9 / 10).copied().unwrap_or(0.0);
    (db(peak), db(rms(&all)), db(p90))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let gain_db: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let speak = args.iter().any(|a| a == "speak");
    let out = std::env::temp_dir().join("eco-mic-probe");
    let _ = std::fs::remove_dir_all(&out);

    match endpoint::get_input_endpoint_volume("default") {
        Ok(v) => println!(
            "Volumen del micrófono en Windows (solo lectura): «{}» nivel real = {:.0} % ({:.4}) silenciado = {} boost = {:?}",
            v.device_name,
            v.level * 100.0,
            v.level,
            v.muted,
            v.boost
        ),
        Err(e) => println!("No se pudo leer el volumen del endpoint: {e}"),
    }

    let sink: eco_audio::EventSink = Arc::new(|_| {});
    let settings = Settings {
        output_dir: out.to_string_lossy().into_owned(),
        mode: Mode::Separate,
        format: Format::Wav32f,
        capture_output: false,
        monitor_when_idle: false,
        input_gain_db: gain_db,
        ..Settings::default()
    };
    let eng = Engine::new(settings, sink, false);
    println!("Ganancia manual: {gain_db:+.0} dB; grabando {secs} s por prueba (habla normal cerca del micrófono)\n");
    println!("{:<12} {:>10} {:>10} {:>14}", "Voz auto.", "pico dBFS", "RMS dBFS", "RMS p90 dBFS");
    for agc in [true, false] {
        let mut s = eng.settings();
        s.input_agc = agc;
        eng.set_settings(s).expect("set_settings");
        eng.start_recording().expect("start_recording");
        let child = speak.then(|| {
            std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
                     Start-Sleep -Milliseconds 300; $s.Speak('Esta es una prueba de grabacion de voz. Uno, dos, tres, cuatro, cinco, seis.')",
                ])
                .spawn()
                .expect("powershell")
        });
        std::thread::sleep(Duration::from_secs(secs));
        if let Some(mut c) = child {
            let _ = c.wait();
        }
        let rec = eng.stop_recording().expect("stop_recording");
        let mic = rec.files.iter().find(|f| f.kind == TrackKind::Input).expect("Microfono.wav");
        let (peak, rms, p90) = stats(Path::new(&mic.path));
        println!("{:<12} {:>10.1} {:>10.1} {:>14.1}   ({})", if agc { "ACTIVADA" } else { "desactivada" }, peak, rms, p90, mic.path);
    }
    eng.shutdown();
}
