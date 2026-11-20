//! Loopback WASAPI del audio del sistema.
//!
//! WASAPI no entrega paquetes de loopback cuando nada suena. Por eso se abre en paralelo un stream de
//! salida sobre el mismo dispositivo que reproduce ceros (keep-alive) y, además, el escritor rellena con
//! silencio cualquier hueco detectado por reloj (`silence_deficit`).

use super::capture::{new_ring, open_input_stream, ErrorSink, Opened, SourceShared};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use std::sync::Arc;

fn build_zeros<T>(device: &cpal::Device, config: &StreamConfig, on_err: ErrorSink) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    device.build_output_stream(
        config,
        |data: &mut [T], _| {
            let z = T::from_sample(0.0f32);
            data.iter_mut().for_each(|s| *s = z);
        },
        move |e| on_err(e.to_string()),
        None,
    )
}

/// Stream de salida que reproduce silencio para mantener vivo el loopback.
fn open_keepalive(device: &cpal::Device, on_err: ErrorSink) -> Result<Stream, String> {
    let cfg = device
        .default_output_config()
        .map_err(|e| format!("keep-alive: {e}"))?;
    let c = cfg.config();
    let r = match cfg.sample_format() {
        SampleFormat::F32 => build_zeros::<f32>(device, &c, on_err),
        SampleFormat::I16 => build_zeros::<i16>(device, &c, on_err),
        SampleFormat::I32 => build_zeros::<i32>(device, &c, on_err),
        SampleFormat::U16 => build_zeros::<u16>(device, &c, on_err),
        SampleFormat::I8 => build_zeros::<i8>(device, &c, on_err),
        SampleFormat::U8 => build_zeros::<u8>(device, &c, on_err),
        SampleFormat::F64 => build_zeros::<f64>(device, &c, on_err),
        other => return Err(format!("keep-alive: formato no soportado {other:?}")),
    };
    let s = r.map_err(|e| format!("keep-alive: {e}"))?;
    s.play().map_err(|e| format!("keep-alive: {e}"))?;
    Ok(s)
}

/// Abre la captura del audio del dispositivo de salida `id` (loopback) + el stream de ceros.
pub fn open_loopback(
    id: &str,
    shared: Arc<SourceShared>,
    on_err: ErrorSink,
) -> Result<Opened, String> {
    let device = super::devices::resolve_output(id)?;
    let name = super::devices::device_name(&device);
    let cfg = device
        .default_output_config()
        .map_err(|e| format!("No se pudo leer el formato de «{name}»: {e}"))?;
    let (sample_rate, channels) = (cfg.sample_rate().0, cfg.channels());
    let (prod, consumer) = new_ring(sample_rate, channels);
    let mut streams = Vec::new();
    // El keep-alive primero: si falla seguimos (el reloj rellenará los huecos).
    let keepalive = match open_keepalive(&device, on_err.clone()) {
        Ok(s) => {
            streams.push(s);
            true
        }
        Err(_) => false,
    };
    // build_input_stream sobre un dispositivo de salida activa el loopback en WASAPI.
    let cap = open_input_stream(&device, cfg.sample_format(), &cfg.config(), shared, prod, on_err)
        .map_err(|e| format!("{e} («{name}»)"))?;
    streams.push(cap);
    Ok(Opened { streams, sample_rate, channels, device_name: name, consumer: Some(consumer), keepalive })
}

/// Cuadros de silencio a insertar para que la pista no se quede atrás del reloj.
/// `expected` = cuadros que deberían existir según el reloj de grabación (sin pausas),
/// `written` = cuadros ya escritos. Solo rellena si el atraso supera `threshold_frames`
/// (para no confundir jitter normal de buffers con un hueco). Con `force` rellena todo el atraso.
pub fn silence_deficit(expected: u64, written: u64, threshold_frames: u64, force: bool) -> u64 {
    let deficit = expected.saturating_sub(written);
    if deficit == 0 || (!force && deficit <= threshold_frames) {
        0
    } else {
        deficit
    }
}

#[cfg(test)]
mod tests {
    use super::silence_deficit;

    #[test]
    fn fills_only_real_gaps() {
        // jitter normal (<= 250 ms a 48k) no se rellena
        assert_eq!(silence_deficit(48_000, 40_000, 12_000, false), 0);
        // hueco de 1 s sí
        assert_eq!(silence_deficit(96_000, 48_000, 12_000, false), 48_000);
        // al detener se completa todo
        assert_eq!(silence_deficit(48_000, 47_000, 12_000, true), 1_000);
        // pista adelantada: nada
        assert_eq!(silence_deficit(48_000, 49_000, 12_000, true), 0);
    }
}
