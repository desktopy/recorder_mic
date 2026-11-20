//! Remuestreo de calidad: sinc con ventana Kaiser (tabla con interpolación lineal), en streaming.
//! Trabaja siempre sobre estéreo intercalado (L,R,L,R…).

const HALF_TAPS: f64 = 24.0; // semiancho del kernel en muestras (a la frecuencia de corte)
const OVER: usize = 512; // resolución de la tabla por unidad
const CUTOFF: f64 = 0.97; // fracción de Nyquist; deja banda de transición
const KAISER_BETA: f64 = 9.0;

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let q = x * x / 4.0;
    for k in 1..60 {
        term *= q / (k as f64 * k as f64);
        sum += term;
        if term < sum * 1e-16 {
            break;
        }
    }
    sum
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Convierte audio intercalado de `channels` canales a estéreo intercalado.
/// mono → duplicar; >2 canales → pares a L, impares a R (promedio).
pub fn to_stereo(data: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => {
            out.reserve(data.len() * 2);
            for &s in data {
                out.push(s);
                out.push(s);
            }
        }
        2 => out.extend_from_slice(data),
        n => {
            out.reserve(data.len() / n * 2);
            let nl = n.div_ceil(2) as f32;
            let nr = (n / 2) as f32;
            for frame in data.chunks_exact(n) {
                let (mut l, mut r) = (0.0, 0.0);
                for (i, &s) in frame.iter().enumerate() {
                    if i % 2 == 0 {
                        l += s;
                    } else {
                        r += s;
                    }
                }
                out.push(l / nl);
                out.push(r / nr);
            }
        }
    }
}

pub struct Resampler {
    in_rate: u32,
    out_rate: u32,
    passthrough: bool,
    step: f64,
    scale: f64,
    width: f64,
    table: Vec<f32>,
    buf: Vec<f32>,
    buf_start: i64,
    n_out: u64,
    total_in: u64,
    finished: bool,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let passthrough = in_rate == out_rate;
        let scale = (out_rate as f64 / in_rate as f64).min(1.0);
        let width = HALF_TAPS / scale;
        let n = (HALF_TAPS * OVER as f64) as usize + 2;
        let i0_beta = bessel_i0(KAISER_BETA);
        let table: Vec<f32> = (0..n)
            .map(|k| {
                let u = k as f64 / OVER as f64; // en unidades normalizadas
                let r = u / HALF_TAPS;
                let win = if r >= 1.0 {
                    0.0
                } else {
                    bessel_i0(KAISER_BETA * (1.0 - r * r).sqrt()) / i0_beta
                };
                (CUTOFF * sinc(CUTOFF * u) * win) as f32
            })
            .collect();
        let pre = width.ceil() as i64 + 1;
        Resampler {
            in_rate,
            out_rate,
            passthrough,
            step: in_rate as f64 / out_rate as f64,
            scale,
            width,
            table,
            buf: vec![0.0; pre as usize * 2],
            buf_start: -pre,
            n_out: 0,
            total_in: 0,
            finished: false,
        }
    }

    pub fn in_rate(&self) -> u32 {
        self.in_rate
    }
    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    #[inline]
    fn weight(&self, x: f64) -> f32 {
        let u = x.abs() * self.scale * OVER as f64;
        let i = u as usize;
        if i + 1 >= self.table.len() {
            return 0.0;
        }
        let f = (u - i as f64) as f32;
        self.table[i] * (1.0 - f) + self.table[i + 1] * f
    }

    /// Empuja estéreo intercalado y agrega a `out` las muestras que ya se pueden producir.
    pub fn process(&mut self, stereo_in: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(stereo_in);
            return;
        }
        self.total_in += (stereo_in.len() / 2) as u64;
        self.buf.extend_from_slice(stereo_in);
        self.produce(out, None);
    }

    /// Vacía la cola (rellena con ceros) hasta completar la duración exacta.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if self.passthrough || self.finished {
            return;
        }
        self.finished = true;
        let pad = self.width.ceil() as usize + 2;
        self.buf.extend(std::iter::repeat_n(0.0, pad * 2));
        let n_total = (self.total_in as u128 * self.out_rate as u128 + (self.in_rate as u128 / 2))
            / self.in_rate as u128;
        self.produce(out, Some(n_total as u64));
    }

    fn produce(&mut self, out: &mut Vec<f32>, limit: Option<u64>) {
        let buf_frames = (self.buf.len() / 2) as i64;
        loop {
            if let Some(l) = limit {
                if self.n_out >= l {
                    break;
                }
            }
            let pos = self.n_out as f64 * self.step;
            let i1 = (pos + self.width).floor() as i64;
            if i1 >= self.buf_start + buf_frames {
                break;
            }
            let i0 = (pos - self.width).ceil() as i64;
            let (mut l, mut r, mut norm) = (0.0f32, 0.0f32, 0.0f32);
            for i in i0..=i1 {
                let w = self.weight(i as f64 - pos);
                let idx = ((i - self.buf_start) * 2) as usize;
                l += w * self.buf[idx];
                r += w * self.buf[idx + 1];
                norm += w;
            }
            if norm.abs() > 1e-9 {
                l /= norm;
                r /= norm;
            }
            out.push(l);
            out.push(r);
            self.n_out += 1;
        }
        // descarta historia que ya no se usará
        let next_i0 = (self.n_out as f64 * self.step - self.width).ceil() as i64 - 1;
        if next_i0 > self.buf_start {
            let drop = ((next_i0 - self.buf_start) as usize).min(self.buf.len() / 2);
            self.buf.drain(..drop * 2);
            self.buf_start += drop as i64;
        }
    }
}

/// Utilidad de una sola pasada: audio intercalado de `channels` canales → estéreo `out_rate`.
pub fn resample_to_stereo(data: &[f32], channels: usize, in_rate: u32, out_rate: u32) -> Vec<f32> {
    let mut st = Vec::new();
    to_stereo(data, channels, &mut st);
    let mut rs = Resampler::new(in_rate, out_rate);
    let mut out = Vec::with_capacity(st.len() * out_rate as usize / in_rate as usize + 16);
    rs.process(&st, &mut out);
    rs.finish(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn mono_44k_to_stereo_48k_keeps_duration() {
        for secs in [1.0f32, 3.37, 0.25] {
            let input = sine(440.0, 44_100, secs, 0.5);
            let out = resample_to_stereo(&input, 1, 44_100, 48_000);
            let frames = out.len() / 2;
            let expected = (input.len() as f64 * 48_000.0 / 44_100.0).round() as i64;
            let tol = (expected as f64 / 1000.0).ceil() as i64;
            assert!(
                (frames as i64 - expected).abs() <= tol.max(1),
                "frames {frames} expected {expected}"
            );
            // mono duplicado: L == R
            assert!(out.chunks(2).all(|f| f[0] == f[1]));
        }
    }

    #[test]
    fn sine_amplitude_and_frequency_preserved() {
        let input = sine(1000.0, 44_100, 1.0, 0.5);
        let out = resample_to_stereo(&input, 1, 44_100, 48_000);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        // ignora bordes
        let mid = &left[2000..left.len() - 2000];
        let rms = (mid.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / mid.len() as f64).sqrt();
        assert!((rms - 0.5 / 2f64.sqrt()).abs() < 0.004, "rms {rms}");
        // frecuencia: cruces por cero ascendentes en 1 s ≈ 1000
        let crossings = mid.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let secs = mid.len() as f64 / 48_000.0;
        let f = crossings as f64 / secs;
        assert!((f - 1000.0).abs() < 8.0, "freq {f}");
        // referencia punto a punto contra la senoide ideal (error < -60 dB)
        let mut max_err = 0.0f64;
        for (n, &y) in left.iter().enumerate().take(left.len() - 1000).skip(1000) {
            let t = n as f64 / 48_000.0;
            let ideal = 0.5 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            max_err = max_err.max((y as f64 - ideal).abs());
        }
        assert!(max_err < 1e-3, "max err {max_err}");
    }

    #[test]
    fn downsample_96k_to_48k_duration_and_alias_rejection() {
        let input = sine(30_000.0, 96_000, 0.5, 0.5); // por encima de Nyquist de salida
        let out = resample_to_stereo(&input, 1, 96_000, 48_000);
        assert_eq!(out.len() / 2, 24_000);
        let mid: Vec<f32> = out.iter().step_by(2).copied().collect();
        let peak = mid[2000..22_000].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak < 0.01, "alias peak {peak}");
    }

    #[test]
    fn streaming_in_odd_chunks_equals_one_shot() {
        let input = sine(523.0, 44_100, 0.7, 0.4);
        let mut st = Vec::new();
        to_stereo(&input, 1, &mut st);
        let one = resample_to_stereo(&input, 1, 44_100, 48_000);
        let mut rs = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for chunk in st.chunks(2 * 777) {
            rs.process(chunk, &mut out);
        }
        rs.finish(&mut out);
        assert_eq!(out.len(), one.len());
        assert!(out.iter().zip(&one).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn passthrough_same_rate_and_multichannel_downmix() {
        let mut st = Vec::new();
        to_stereo(&[0.1, 0.2, 0.3, 0.4], 2, &mut st);
        assert_eq!(st, vec![0.1, 0.2, 0.3, 0.4]);
        let out = resample_to_stereo(&[0.5; 48], 1, 48_000, 48_000);
        assert_eq!(out.len(), 96);
        let mut six = Vec::new();
        to_stereo(&[1.0, 0.0, 1.0, 0.0, 1.0, 0.0], 6, &mut six);
        assert_eq!(six, vec![1.0, 0.0]);
    }
}
