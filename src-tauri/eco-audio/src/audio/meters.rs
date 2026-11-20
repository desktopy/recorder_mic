//! Medidores: acumulación sin locks en el callback y lectura (con caída de pico) en el hilo de eventos.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

/// Lado del callback de audio: solo atómicos, sin locks ni E/S.
pub struct MeterAccum {
    peak_bits: AtomicU32,
    sumsq_bits: AtomicU64,
    count: AtomicU32,
}

impl Default for MeterAccum {
    fn default() -> Self {
        Self::new()
    }
}

impl MeterAccum {
    pub const fn new() -> Self {
        MeterAccum {
            peak_bits: AtomicU32::new(0),
            sumsq_bits: AtomicU64::new(0),
            count: AtomicU32::new(0),
        }
    }

    /// Publica pico y energía de un bloque de muestras (ya con ganancia).
    pub fn update(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let (rms_sq_sum, peak) = sumsq_peak(samples);
        // Los floats no negativos se ordenan igual que sus bits.
        self.peak_bits.fetch_max(peak.to_bits(), Relaxed);
        let mut cur = self.sumsq_bits.load(Relaxed);
        loop {
            let new = (f64::from_bits(cur) + rms_sq_sum).to_bits();
            match self
                .sumsq_bits
                .compare_exchange_weak(cur, new, Relaxed, Relaxed)
            {
                Ok(_) => break,
                Err(c) => cur = c,
            }
        }
        self.count.fetch_add(samples.len() as u32, Relaxed);
    }

    /// Toma (y reinicia) la ventana acumulada: `(rms, peak, muestras)`.
    pub fn take(&self) -> (f32, f32, u32) {
        let peak = f32::from_bits(self.peak_bits.swap(0, Relaxed));
        let sumsq = f64::from_bits(self.sumsq_bits.swap(0, Relaxed));
        let n = self.count.swap(0, Relaxed);
        let rms = if n > 0 { (sumsq / n as f64).sqrt() as f32 } else { 0.0 };
        (rms, peak, n)
    }
}

fn sumsq_peak(samples: &[f32]) -> (f64, f32) {
    let mut s = 0.0f64;
    let mut p = 0.0f32;
    for &x in samples {
        let a = x.abs();
        if a.is_finite() {
            s += (a as f64) * (a as f64);
            if a > p {
                p = a;
            }
        }
    }
    (s, p)
}

/// RMS y pico de un bloque (utilidad pura, usada en tests y en la sonda).
pub fn rms_peak(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let (s, p) = sumsq_peak(samples);
    (((s / samples.len() as f64).sqrt()) as f32, p)
}

/// Lado del lector: el pico cae suavemente entre lecturas para que la UI no parpadee.
pub struct MeterReader {
    held_peak: f32,
    decay: f32,
}

impl MeterReader {
    pub fn new(decay_per_tick: f32) -> Self {
        MeterReader { held_peak: 0.0, decay: decay_per_tick }
    }

    /// Devuelve `(rms, peak)` en 0..1.
    pub fn read(&mut self, acc: &MeterAccum) -> (f32, f32) {
        let (rms, peak, _n) = acc.take();
        self.held_peak = peak.max(self.held_peak * self.decay);
        if self.held_peak < 1e-5 {
            self.held_peak = 0.0;
        }
        (rms.min(1.0), self.held_peak.min(1.0))
    }

    pub fn reset(&mut self) {
        self.held_peak = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_peak_of_sine() {
        let v: Vec<f32> = (0..4800)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48000.0).sin())
            .collect();
        let (r, p) = rms_peak(&v);
        assert!((r - 0.5 / 2f32.sqrt()).abs() < 0.005, "rms {r}");
        assert!((p - 0.5).abs() < 0.01, "peak {p}");
    }

    #[test]
    fn accum_take_resets_and_reader_decays() {
        let acc = MeterAccum::new();
        acc.update(&[0.5, -0.5, 0.5, -0.5]);
        let mut rd = MeterReader::new(0.5);
        let (r, p) = rd.read(&acc);
        assert!((r - 0.5).abs() < 1e-6 && (p - 0.5).abs() < 1e-6);
        let (r2, p2) = rd.read(&acc);
        assert_eq!(r2, 0.0);
        assert!((p2 - 0.25).abs() < 1e-6);
    }
}
