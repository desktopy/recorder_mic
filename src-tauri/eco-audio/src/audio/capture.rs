//! Captura: estado compartido por fuente + construcción de streams de entrada.
//! El callback de audio solo usa atómicos y un ring buffer SPSC (sin locks, sin E/S, sin asignaciones).

use super::dsp::VoiceProcessor;
use super::meters::MeterAccum;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use ringbuf::traits::{Observer, Producer};
use ringbuf::traits::Split;
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::*};
use std::sync::Arc;
use std::time::Instant;

pub const UNSET: u64 = u64::MAX;

/// Estado de una fuente (micrófono o loopback) compartido entre callback, escritor y control.
pub struct SourceShared {
    pub epoch: Instant,
    gain_bits: AtomicU32,
    /// «Voz automática» (AGC) activa para esta fuente.
    agc: AtomicBool,
    pub meter: MeterAccum,
    /// Hay un stream abierto para esta fuente.
    pub open: AtomicBool,
    /// Se está grabando (los callbacks empujan al ring).
    pub recording: AtomicBool,
    pub paused: AtomicBool,
    /// Instante (ns desde `epoch`) en que empezó la grabación.
    pub rec_start_ns: AtomicU64,
    /// Instante (ns desde `epoch`) del primer frame empujado al ring en esta grabación.
    pub first_frame_ns: AtomicU64,
    /// Muestras descartadas por ring lleno.
    pub dropped: AtomicU64,
}

impl SourceShared {
    pub fn new(epoch: Instant) -> Self {
        SourceShared {
            epoch,
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            agc: AtomicBool::new(false),
            meter: MeterAccum::new(),
            open: AtomicBool::new(false),
            recording: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            rec_start_ns: AtomicU64::new(0),
            first_frame_ns: AtomicU64::new(UNSET),
            dropped: AtomicU64::new(0),
        }
    }
    pub fn set_gain(&self, g: f32) {
        self.gain_bits.store(g.to_bits(), Relaxed);
    }
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Relaxed))
    }
    pub fn set_agc(&self, on: bool) {
        self.agc.store(on, Relaxed);
    }
    pub fn agc(&self) -> bool {
        self.agc.load(Relaxed)
    }
    pub fn now_ns(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }
}

pub type ErrorSink = Arc<dyn Fn(String) + Send + Sync>;

pub const RING_SECONDS: usize = 8;

/// Ring SPSC con capacidad para `RING_SECONDS` de audio nativo.
pub fn new_ring(sample_rate: u32, channels: u16) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new(sample_rate as usize * channels.max(1) as usize * RING_SECONDS).split()
}

fn build_typed<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<SourceShared>,
    mut prod: HeapProd<f32>,
    on_err: ErrorSink,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let ch = config.channels.max(1) as usize;
    let sr = config.sample_rate.0.max(1) as u64;
    // Múltiplo de `channels` para no partir cuadros entre trozos.
    let chunk = (2048 / ch).max(1) * ch;
    // Cadena de voz (ganancia manual → AGC → limitador): reserva su memoria aquí, no en el callback.
    let mut dsp = VoiceProcessor::new(config.sample_rate.0, config.channels);
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            let gain = shared.gain();
            let agc = shared.agc();
            let rec = shared.recording.load(Acquire) && !shared.paused.load(Acquire);
            if rec && shared.first_frame_ns.load(Relaxed) == UNSET {
                let dur = (data.len() / ch) as u64 * 1_000_000_000 / sr;
                let first = shared.now_ns().saturating_sub(dur);
                let _ = shared.first_frame_ns.compare_exchange(UNSET, first, AcqRel, Relaxed);
            }
            let mut buf = [0.0f32; 2048];
            for part in data.chunks(chunk) {
                for (o, i) in buf.iter_mut().zip(part) {
                    *o = <f32 as FromSample<T>>::from_sample_(*i);
                }
                // Los medidores y el WAV reciben la señal ya procesada.
                dsp.process(&mut buf[..part.len()], gain, agc);
                let b = &buf[..part.len()];
                shared.meter.update(b);
                if rec {
                    // solo cuadros completos
                    let can = (prod.vacant_len().min(b.len()) / ch) * ch;
                    if can > 0 {
                        prod.push_slice(&b[..can]);
                    }
                    if can < b.len() {
                        shared.dropped.fetch_add((b.len() - can) as u64, Relaxed);
                    }
                }
            }
        },
        move |e| on_err(e.to_string()),
        None,
    )
}

/// Abre un stream de entrada sobre `device` (si es un dispositivo de salida, WASAPI lo abre como loopback).
pub fn open_input_stream(
    device: &cpal::Device,
    format: SampleFormat,
    config: &StreamConfig,
    shared: Arc<SourceShared>,
    prod: HeapProd<f32>,
    on_err: ErrorSink,
) -> Result<Stream, String> {
    let r = match format {
        SampleFormat::F32 => build_typed::<f32>(device, config, shared, prod, on_err),
        SampleFormat::I16 => build_typed::<i16>(device, config, shared, prod, on_err),
        SampleFormat::I32 => build_typed::<i32>(device, config, shared, prod, on_err),
        SampleFormat::U16 => build_typed::<u16>(device, config, shared, prod, on_err),
        SampleFormat::I8 => build_typed::<i8>(device, config, shared, prod, on_err),
        SampleFormat::U8 => build_typed::<u8>(device, config, shared, prod, on_err),
        SampleFormat::F64 => build_typed::<f64>(device, config, shared, prod, on_err),
        other => return Err(format!("Formato de muestra no soportado: {other:?}")),
    };
    let stream = r.map_err(|e| format!("No se pudo abrir la captura: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("No se pudo iniciar la captura: {e}"))?;
    Ok(stream)
}

/// Streams abiertos de una fuente y su formato nativo.
pub struct Opened {
    /// Se conservan vivos mientras la fuente esté abierta (el orden de drop no importa).
    pub streams: Vec<Stream>,
    pub sample_rate: u32,
    pub channels: u16,
    pub device_name: String,
    /// Extremo lector del ring (lo consume el hilo escritor mientras se graba).
    pub consumer: Option<HeapCons<f32>>,
    /// Solo loopback: hay un stream de ceros keep-alive en el mismo dispositivo.
    pub keepalive: bool,
}

/// Abre el micrófono `id` en su formato nativo.
pub fn open_microphone(
    id: &str,
    shared: Arc<SourceShared>,
    on_err: ErrorSink,
) -> Result<Opened, String> {
    let device = super::devices::resolve_input(id)?;
    let name = super::devices::device_name(&device);
    let cfg = device
        .default_input_config()
        .map_err(|e| format!("No se pudo leer el formato de «{name}»: {e}"))?;
    let (sample_rate, channels) = (cfg.sample_rate().0, cfg.channels());
    let (prod, consumer) = new_ring(sample_rate, channels);
    let stream = open_input_stream(&device, cfg.sample_format(), &cfg.config(), shared, prod, on_err)
        .map_err(|e| format!("{e} («{name}»)"))?;
    Ok(Opened { streams: vec![stream], sample_rate, channels, device_name: name, consumer: Some(consumer), keepalive: false })
}
