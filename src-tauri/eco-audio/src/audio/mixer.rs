//! Mezcla offline: cada pista → 48 kHz estéreo (sinc de calidad), alineadas por el instante de su
//! primer frame, sumadas con ganancia y pasadas por un limitador suave. Todo en streaming (memoria constante).

use super::dsp::Ducker;
use super::resample::{to_stereo, Resampler};
use super::wav::{TrackReader, TrackWriter};
use crate::types::Format;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub const MIX_RATE: u32 = 48_000;
const BLOCK_FRAMES: usize = 4800;

/// Techo del limitador (< 1.0 para no tocar nunca la escala completa) y rodilla donde empieza a actuar.
const CEILING: f32 = 0.995;
const KNEE: f32 = 0.8;

/// Limitador suave memoryless: lineal hasta la rodilla, luego compresión tanh hacia el techo.
/// Garantiza |y| < 1.0 para cualquier entrada (NaN → 0).
#[inline]
pub fn soft_limit(x: f32) -> f32 {
    if x.is_nan() {
        return 0.0;
    }
    let a = x.abs();
    if a <= KNEE {
        return x;
    }
    let y = KNEE + (CEILING - KNEE) * ((a - KNEE) / (CEILING - KNEE)).tanh();
    y.copysign(x)
}

/// Fuente de audio estéreo a 48 kHz.
pub trait StereoSource {
    /// Rellena `out` (se limpia antes) con hasta `frames` cuadros intercalados.
    /// Debe devolver exactamente `frames` salvo al final del audio; vacío = fin.
    fn read(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), String>;
}

/// Fuente en memoria (ya estéreo 48k). Útil para tests.
pub struct MemSource {
    data: Vec<f32>,
    pos: usize,
}
impl MemSource {
    pub fn new(data: Vec<f32>) -> Self {
        MemSource { data, pos: 0 }
    }
}
impl StereoSource for MemSource {
    fn read(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), String> {
        out.clear();
        let end = (self.pos + frames * 2).min(self.data.len());
        out.extend_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(())
    }
}

/// Fuente desde un WAV en disco, remuestreada a 48k estéreo.
pub struct WavSource {
    reader: TrackReader,
    channels: usize,
    rs: Resampler,
    pending: VecDeque<f32>,
    raw: Vec<f32>,
    stereo: Vec<f32>,
    out: Vec<f32>,
    eof: bool,
}

impl WavSource {
    pub fn open(path: &Path) -> Result<Self, String> {
        let reader = TrackReader::open(path)?;
        let spec = reader.spec;
        Ok(WavSource {
            channels: spec.channels.max(1) as usize,
            rs: Resampler::new(spec.sample_rate.max(1), MIX_RATE),
            reader,
            pending: VecDeque::new(),
            raw: Vec::new(),
            stereo: Vec::new(),
            out: Vec::new(),
            eof: false,
        })
    }
}

impl StereoSource for WavSource {
    fn read(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), String> {
        out.clear();
        let want = frames * 2;
        while self.pending.len() < want && !self.eof {
            self.reader.read_samples(4096 * self.channels, &mut self.raw);
            // descarta un posible cuadro incompleto al final de un archivo truncado
            let usable = self.raw.len() - self.raw.len() % self.channels;
            self.out.clear();
            if usable == 0 {
                self.rs.finish(&mut self.out);
                self.eof = true;
            } else {
                self.stereo.clear();
                to_stereo(&self.raw[..usable], self.channels, &mut self.stereo);
                self.rs.process(&self.stereo, &mut self.out);
            }
            self.pending.extend(self.out.iter().copied());
        }
        let n = want.min(self.pending.len());
        out.extend(self.pending.drain(..n));
        Ok(())
    }
}

/// Papel de una pista en la mezcla (prioridad de voz).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrackRole {
    #[default]
    Normal,
    /// Micrófono: su voz hace bajar las pistas `Ducked`.
    Voice,
    /// Audio del sistema: baja ~8 dB mientras hay voz.
    Ducked,
}

pub struct MixTrack {
    pub source: Box<dyn StereoSource>,
    pub role: TrackRole,
    /// Posición del primer frame de la pista en la línea de tiempo común (cuadros a 48 kHz).
    pub offset_frames: u64,
    pub gain: f32,
}

/// Mezcla las pistas y entrega bloques ya limitados al `sink`. Devuelve los cuadros totales.
/// La línea de tiempo empieza en el primer frame de la pista más temprana.
pub fn mix_tracks(
    tracks: Vec<MixTrack>,
    mut sink: impl FnMut(&[f32]) -> Result<(), String>,
) -> Result<u64, String> {
    let base = tracks.iter().map(|t| t.offset_frames).min().unwrap_or(0);
    struct St {
        src: Box<dyn StereoSource>,
        role: TrackRole,
        delay_left: u64,
        gain: f32,
        done: bool,
        /// Cuadros de silencio al inicio del bloque y audio leído (estéreo).
        pad: usize,
        buf: Vec<f32>,
    }
    let mut st: Vec<St> = tracks
        .into_iter()
        .map(|t| St {
            src: t.source,
            role: t.role,
            delay_left: t.offset_frames - base,
            gain: t.gain,
            done: false,
            pad: 0,
            buf: Vec::new(),
        })
        .collect();
    let duck = st.iter().any(|t| t.role == TrackRole::Voice) && st.iter().any(|t| t.role == TrackRole::Ducked);
    let mut ducker = Ducker::new(MIX_RATE);
    let mut duck_gain: Vec<f32> = Vec::new();
    let mut voice = vec![0.0f32; BLOCK_FRAMES * 2];
    let mut total: u64 = 0;
    let mut acc = vec![0.0f32; BLOCK_FRAMES * 2];
    loop {
        acc.iter_mut().for_each(|x| *x = 0.0);
        let mut block_len = 0usize;
        for t in st.iter_mut() {
            t.buf.clear();
            t.pad = 0;
            if t.done {
                continue;
            }
            let pad = (t.delay_left.min(BLOCK_FRAMES as u64)) as usize;
            t.delay_left -= pad as u64;
            if t.delay_left > 0 {
                block_len = BLOCK_FRAMES; // aún en el silencio previo a su primer frame
                continue;
            }
            let need = BLOCK_FRAMES - pad;
            if need > 0 {
                t.src.read(need, &mut t.buf)?;
            }
            let got = t.buf.len() / 2;
            if got < need {
                t.done = true;
            }
            t.pad = pad;
            block_len = block_len.max(pad + got);
        }
        if block_len == 0 {
            break;
        }
        if duck {
            // Voz del micrófono del bloque (silencio donde no hay audio) → ganancia por cuadro para el sistema.
            voice[..block_len * 2].iter_mut().for_each(|x| *x = 0.0);
            for t in st.iter().filter(|t| t.role == TrackRole::Voice) {
                for (v, s) in voice[t.pad * 2..].iter_mut().zip(&t.buf) {
                    *v += s;
                }
            }
            ducker.gains(&voice[..block_len * 2], 2, &mut duck_gain);
        }
        for t in st.iter() {
            let ducked = duck && t.role == TrackRole::Ducked;
            for f in 0..t.buf.len() / 2 {
                let g = if ducked { t.gain * duck_gain[t.pad + f] } else { t.gain };
                let o = (t.pad + f) * 2;
                acc[o] += t.buf[f * 2] * g;
                acc[o + 1] += t.buf[f * 2 + 1] * g;
            }
        }
        let blk = &mut acc[..block_len * 2];
        blk.iter_mut().for_each(|x| *x = soft_limit(*x));
        sink(blk)?;
        total += block_len as u64;
        if st.iter().all(|t| t.done) {
            break;
        }
    }
    Ok(total)
}

/// Mezcla en memoria (tests): pistas estéreo 48k con offset (cuadros) y ganancia.
pub fn mix_buffers(tracks: Vec<(Vec<f32>, u64, f32)>) -> Vec<f32> {
    let tr = tracks
        .into_iter()
        .map(|(d, o, g)| MixTrack { source: Box::new(MemSource::new(d)), role: TrackRole::Normal, offset_frames: o, gain: g })
        .collect();
    let mut out = Vec::new();
    mix_tracks(tr, |b| {
        out.extend_from_slice(b);
        Ok(())
    })
    .expect("mix en memoria");
    out
}

pub struct MixInput {
    pub path: PathBuf,
    /// Instante del primer frame respecto al inicio de la sesión, en nanosegundos.
    pub offset_ns: u64,
    pub gain: f32,
    pub role: TrackRole,
}

pub fn ns_to_frames(ns: u64) -> u64 {
    ((ns as u128 * MIX_RATE as u128 + 500_000_000) / 1_000_000_000) as u64
}

/// Mezcla WAVs de disco en `out` (48 kHz estéreo, `format`). Devuelve la duración en cuadros.
pub fn mix_files(inputs: &[MixInput], out: &Path, format: Format) -> Result<u64, String> {
    if inputs.is_empty() {
        return Err("No hay pistas para mezclar.".into());
    }
    let mut tracks = Vec::new();
    for i in inputs {
        tracks.push(MixTrack {
            source: Box::new(WavSource::open(&i.path)?),
            role: i.role,
            offset_frames: ns_to_frames(i.offset_ns),
            gain: i.gain,
        });
    }
    let mut w = TrackWriter::create(out, MIX_RATE, 2, format)?;
    let r = mix_tracks(tracks, |b| w.write(b));
    match r {
        Ok(frames) => {
            w.finalize()?;
            Ok(frames)
        }
        Err(e) => {
            let _ = w.finalize();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::resample::resample_to_stereo;

    #[test]
    fn limiter_never_exceeds_one_and_is_transparent_below_knee() {
        let mut worst = 0.0f32;
        let mut x = -50.0f32;
        while x <= 50.0 {
            let y = soft_limit(x);
            worst = worst.max(y.abs());
            assert!(y.abs() <= 1.0, "x={x} y={y}");
            x += 0.0137;
        }
        for v in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, f32::MAX, f32::MIN] {
            let y = soft_limit(v);
            assert!(y.abs() <= 1.0 && !y.is_nan(), "{v} -> {y}");
        }
        assert!(worst < 1.0);
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.79), -0.79);
        // monótono
        let mut prev = soft_limit(-5.0);
        let mut v = -5.0f32;
        while v < 5.0 {
            v += 0.001;
            let y = soft_limit(v);
            assert!(y >= prev - 1e-7);
            prev = y;
        }
    }

    #[test]
    fn mixed_output_never_clips() {
        let loud: Vec<f32> = (0..9600).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = mix_buffers(vec![(loud.clone(), 0, 2.0), (loud, 0, 2.0)]);
        assert_eq!(out.len(), 9600);
        assert!(out.iter().all(|x| x.abs() <= 1.0));
    }

    #[test]
    fn alignment_with_offset_and_gain() {
        // A: impulso 0.5 en el cuadro 0. B: impulso 0.25 en su cuadro 0, que ocurre 4800 cuadros después.
        let mut a = vec![0.0f32; 2 * 10_000];
        a[0] = 0.5;
        a[1] = 0.5;
        let mut b = vec![0.0f32; 2 * 3000];
        b[0] = 0.25;
        b[1] = 0.25;
        for (oa, ob) in [(0u64, 4800u64), (1000, 5800)] {
            let out = mix_buffers(vec![(a.clone(), oa, 1.0), (b.clone(), ob, 1.0)]);
            assert_eq!(out.len(), 2 * 10_000, "largo = fin de la pista más larga");
            assert_eq!(out[0], 0.5);
            assert_eq!(out[2 * 4800], 0.25, "B cae exactamente en el cuadro 4800");
            assert_eq!(out[2 * 4800 + 1], 0.25);
            assert_eq!(out[2 * 4799], 0.0);
        }
        // B empieza antes que A: la línea de tiempo arranca en B
        let out = mix_buffers(vec![(a.clone(), 480, 0.5), (b.clone(), 0, 1.0)]);
        assert_eq!(out[0], 0.25);
        assert_eq!(out[2 * 480], 0.25); // A·0.5
        // el largo cubre offset + duración de A
        assert_eq!(out.len(), 2 * (480 + 10_000));
        // pista que termina antes del arranque de la otra: hueco de silencio
        let out = mix_buffers(vec![(vec![0.5; 2 * 100], 0, 1.0), (vec![0.5; 2 * 100], 9_000, 1.0)]);
        assert_eq!(out.len(), 2 * 9_100);
        assert_eq!(out[2 * 500], 0.0);
        assert_eq!(out[2 * 9_000], 0.5);
    }

    #[test]
    fn voice_priority_ducks_system_in_the_mix() {
        let sr = MIX_RATE as usize;
        // Sistema: seno a -20 dBFS, 6 s estéreo. Micrófono: 2 s de voz (ruido -25 dBFS) entre 1 s y 3 s.
        let sys: Vec<f32> = (0..sr * 6)
            .flat_map(|i| {
                let v = 0.1 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / sr as f32).sin();
                [v, v]
            })
            .collect();
        let mut seed = 0x1234_5678u32;
        let mic: Vec<f32> = (0..sr * 6)
            .flat_map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let n = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                let v = if (sr..sr * 3).contains(&i) { n * 0.05 * 3f32.sqrt() } else { 0.0 };
                [v, v]
            })
            .collect();
        let mix = |roles: [TrackRole; 2]| {
            let tr = vec![
                MixTrack { source: Box::new(MemSource::new(mic.clone())), role: roles[0], offset_frames: 0, gain: 0.0 },
                MixTrack { source: Box::new(MemSource::new(sys.clone())), role: roles[1], offset_frames: 0, gain: 1.0 },
            ];
            let mut out = Vec::new();
            mix_tracks(tr, |b| {
                out.extend_from_slice(b);
                Ok(())
            })
            .unwrap();
            out
        };
        // gain 0 en el micrófono: solo se oye el sistema, así se mide su ducking aislado.
        let plain = mix([TrackRole::Normal, TrackRole::Normal]);
        let ducked = mix([TrackRole::Voice, TrackRole::Ducked]);
        let rms = |x: &[f32], a: f32, b: f32| {
            let s = &x[(a * sr as f32) as usize * 2..(b * sr as f32) as usize * 2];
            (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
        };
        let db = |a: f32, b: f32| 20.0 * (rms(&ducked, a, b) / rms(&plain, a, b)).log10();
        let (before, during, after) = (db(0.2, 0.9), db(1.8, 2.9), db(5.0, 6.0));
        println!("ducking en mezcla: antes {before:.2} dB, durante {during:.2} dB, después {after:.2} dB");
        assert!(before > -0.5, "{before}");
        assert!(during <= -6.0, "{during}");
        assert!(after > -1.0, "{after}");
        assert_eq!(plain.len(), ducked.len());
    }

    #[test]
    fn ns_to_frames_rounds() {
        assert_eq!(ns_to_frames(0), 0);
        assert_eq!(ns_to_frames(1_000_000_000), 48_000);
        assert_eq!(ns_to_frames(100_000_000), 4_800);
    }

    #[test]
    fn mix_files_end_to_end_resamples_and_keeps_duration() {
        let dir = std::env::temp_dir().join("eco-mix-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // pista 1: mono 44.1k, 2 s; pista 2: estéreo 48k, 1 s, empieza 0.5 s más tarde
        let p1 = dir.join("a.wav");
        let p2 = dir.join("b.wav");
        let s1: Vec<f32> = (0..88_200).map(|i| 0.3 * (i as f32 * 0.06).sin()).collect();
        let mut w = TrackWriter::create(&p1, 44_100, 1, Format::Wav32f).unwrap();
        w.write(&s1).unwrap();
        w.finalize().unwrap();
        let mut w = TrackWriter::create(&p2, 48_000, 2, Format::Wav24).unwrap();
        w.write(&vec![0.2f32; 96_000]).unwrap();
        w.finalize().unwrap();
        let out = dir.join("m.wav");
        let frames = mix_files(
            &[
                MixInput { path: p1, offset_ns: 0, gain: 1.0, role: TrackRole::Normal },
                MixInput { path: p2, offset_ns: 500_000_000, gain: 1.0, role: TrackRole::Normal },
            ],
            &out,
            Format::Wav24,
        )
        .unwrap();
        assert!((frames as i64 - 96_000).abs() <= 96, "frames {frames}");
        let (sr, ch, dur, peak) = crate::audio::wav::analyze(&out).unwrap();
        assert_eq!((sr, ch), (48_000, 2));
        assert!((dur - 2.0).abs() < 0.002, "dur {dur}");
        assert!(peak <= 1.0 && peak > 0.3);
        // la resampleada de referencia coincide en duración
        let r = resample_to_stereo(&s1, 1, 44_100, 48_000);
        assert!((r.len() as i64 / 2 - 96_000).abs() <= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
