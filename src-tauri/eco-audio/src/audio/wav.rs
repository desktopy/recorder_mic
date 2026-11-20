//! Escritura/lectura de WAV con `hound`. Solo se usa desde el hilo escritor y desde la mezcla (nunca desde un callback).

use crate::types::Format;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub struct TrackWriter {
    w: Option<WavWriter<BufWriter<File>>>,
    mp3: Option<crate::library::StreamingMp3>,
    format: Format,
    channels: u16,
    pub frames: u64,
}

fn spec_for(format: Format, channels: u16, sample_rate: u32) -> WavSpec {
    match format {
        Format::Wav16 => WavSpec { channels, sample_rate, bits_per_sample: 16, sample_format: SampleFormat::Int },
        Format::Wav24 | Format::Mp3 => WavSpec { channels, sample_rate, bits_per_sample: 24, sample_format: SampleFormat::Int },
        Format::Wav32f => WavSpec { channels, sample_rate, bits_per_sample: 32, sample_format: SampleFormat::Float },
    }
}

#[inline]
fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}
#[inline]
fn to_i24(x: f32) -> i32 {
    (x.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32
}

impl TrackWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16, format: Format) -> Result<Self, String> {
        let file = File::options().write(true).create_new(true).open(path).map_err(|e| format!("No se pudo crear «{}»: {e}", path.display()))?;
        let w = WavWriter::new(BufWriter::with_capacity(256 * 1024, file), spec_for(format, channels, sample_rate))
            .map_err(|e| format!("No se pudo iniciar el WAV: {e}"))?;
        Ok(TrackWriter { w: Some(w), mp3: None, format, channels, frames: 0 })
    }

    pub fn direct_mp3(file: File, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let mp3 = crate::library::StreamingMp3::new(file, sample_rate, channels)?;
        Ok(Self { w: None, mp3: Some(mp3), format: Format::Mp3, channels, frames: 0 })
    }

    /// Escribe muestras intercaladas (múltiplo de `channels`).
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        if let Some(mp3) = &mut self.mp3 {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mp3.write(samples)))
                .unwrap_or_else(|_| Err("Внутренняя ошибка потокового MP3-кодировщика".into()))?;
            self.frames += (samples.len() / self.channels.max(1) as usize) as u64;
            return Ok(());
        }
        let w = self.w.as_mut().ok_or("WAV ya cerrado")?;
        let r = match self.format {
            Format::Wav16 => samples.iter().try_for_each(|&s| w.write_sample(to_i16(s))),
            Format::Wav24 | Format::Mp3 => samples.iter().try_for_each(|&s| w.write_sample(to_i24(s))),
            Format::Wav32f => samples.iter().try_for_each(|&s| w.write_sample(s)),
        };
        r.map_err(|e| format!("Error al escribir el audio en disco: {e}"))?;
        self.frames += (samples.len() / self.channels.max(1) as usize) as u64;
        Ok(())
    }

    pub fn write_silence(&mut self, frames: u64) -> Result<(), String> {
        let zeros = vec![0.0f32; 4096 * self.channels.max(1) as usize];
        let mut left = frames;
        while left > 0 {
            let n = left.min(4096);
            self.write(&zeros[..n as usize * self.channels.max(1) as usize])?;
            left -= n;
        }
        Ok(())
    }

    /// Vuelca buffers y actualiza la cabecera para que el archivo sea válido aunque se corte el proceso.
    pub fn flush(&mut self) -> Result<(), String> {
        if let Some(mp3) = &mut self.mp3 { return mp3.flush(); }
        if let Some(w) = self.w.as_mut() {
            w.flush().map_err(|e| format!("Error al volcar el audio: {e}"))?;
        }
        Ok(())
    }

    pub fn finalize(mut self) -> Result<u64, String> {
        if let Some(mp3) = self.mp3.take() {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mp3.finish()))
                .unwrap_or_else(|_| Err("Ошибка финализации MP3; полученные байты не удалены".into()))?;
        }
        if let Some(w) = self.w.take() {
            w.finalize().map_err(|e| format!("Error al cerrar el WAV: {e}"))?;
        }
        Ok(self.frames)
    }
}

/// Lector de WAV que entrega muestras como f32 en -1..1.
pub struct TrackReader {
    reader: WavReader<std::io::BufReader<File>>,
    pub spec: WavSpec,
}

impl TrackReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        let reader = WavReader::open(path).map_err(|e| format!("No se pudo abrir «{}»: {e}", path.display()))?;
        let spec = reader.spec();
        Ok(TrackReader { reader, spec })
    }

    /// Lee hasta `n` muestras; devuelve las que haya (vacío = fin o archivo truncado).
    pub fn read_samples(&mut self, n: usize, out: &mut Vec<f32>) {
        out.clear();
        match (self.spec.sample_format, self.spec.bits_per_sample) {
            (SampleFormat::Float, _) => {
                for s in self.reader.samples::<f32>().take(n) {
                    match s {
                        Ok(v) => out.push(v),
                        Err(_) => break,
                    }
                }
            }
            (SampleFormat::Int, bits) => {
                let scale = 1.0 / (1u64 << (bits.clamp(1, 32) - 1)) as f32;
                for s in self.reader.samples::<i32>().take(n) {
                    match s {
                        Ok(v) => out.push(v as f32 * scale),
                        Err(_) => break,
                    }
                }
            }
        }
    }
}

/// Duración y pico de un WAV (para la sonda y diagnósticos).
pub fn analyze(path: &Path) -> Result<(u32, u16, f64, f32), String> {
    let mut r = TrackReader::open(path)?;
    let spec = r.spec;
    let mut buf = Vec::new();
    let mut total: u64 = 0;
    let mut peak = 0.0f32;
    loop {
        r.read_samples(48_000, &mut buf);
        if buf.is_empty() {
            break;
        }
        total += buf.len() as u64;
        peak = buf.iter().fold(peak, |a, b| a.max(b.abs()));
    }
    let frames = total / spec.channels.max(1) as u64;
    Ok((spec.sample_rate, spec.channels, frames as f64 / spec.sample_rate as f64, peak))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_formats() {
        let dir = std::env::temp_dir().join("eco-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        for (fmt, tol) in [(Format::Wav16, 1e-4), (Format::Wav24, 1e-6), (Format::Wav32f, 0.0)] {
            let p = dir.join(format!("t-{fmt:?}.wav"));
            let src: Vec<f32> = (0..2000).map(|i| 0.9 * (i as f32 * 0.05).sin()).collect();
            let mut w = TrackWriter::create(&p, 44_100, 2, fmt).unwrap();
            w.write(&src).unwrap();
            w.write_silence(100).unwrap();
            assert_eq!(w.finalize().unwrap(), 1100);
            let mut r = TrackReader::open(&p).unwrap();
            assert_eq!(r.spec.sample_rate, 44_100);
            let mut got = Vec::new();
            r.read_samples(10_000, &mut got);
            assert_eq!(got.len(), 2200);
            for (a, b) in src.iter().zip(&got) {
                assert!((a - b).abs() <= tol + 1e-9, "{fmt:?}: {a} vs {b}");
            }
            assert!(got[2000..].iter().all(|&x| x == 0.0));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
