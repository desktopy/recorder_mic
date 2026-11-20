//! DSP de voz: detector de voz con piso de ruido adaptable, control automático de ganancia (AGC),
//! limitador look-ahead y ducking de la mezcla.
//!
//! Todo es de coste constante por muestra y **no asigna memoria ni toma locks** una vez creado, así que
//! puede correr dentro del callback de captura sin bloquearlo (la memoria se reserva al construir).
//! El control (detector, ganancias) corre por bloques de `CTRL` cuadros; la ganancia se interpola por muestra.

use std::collections::VecDeque;

/// Techo del limitador: -1 dBFS.
pub const CEILING: f32 = 0.891_25;
/// Objetivo del AGC: RMS de voz.
pub const AGC_TARGET_DB: f32 = -18.0;
/// El detector de nivel (ataque rápido) rinde algo por encima del RMS real; se compensa para caer en el objetivo.
const LEVEL_BIAS_DB: f32 = 1.5;
pub const AGC_MAX_DB: f32 = 30.0;
pub const AGC_MIN_DB: f32 = -20.0;
/// Cuadros por bloque de control (~0,7 ms a 48 kHz).
pub const CTRL: usize = 32;

const ABS_OPEN_DB: f32 = -66.0;
const ABS_CLOSE_DB: f32 = -69.0;
const OPEN_ABOVE_FLOOR_DB: f32 = 8.0;
const CLOSE_ABOVE_FLOOR_DB: f32 = 5.0;
const WARMUP_S: f32 = 0.3;
const HANG_S: f32 = 0.25;
const FLOOR_RISE_DB_PER_S: f32 = 1.5;
const FLOOR_MIN_DB: f32 = -100.0;
const SPEECH_WINDOW_S: f32 = 0.5;
const SPEECH_FORGET_S: f32 = 1.0;
/// Mientras no se haya comprobado que es habla, el AGC no sube por encima de esta ganancia.
const UNPROVEN_MAX_DB: f32 = 6.0;
const STAT_WINDOW_S: f32 = 0.6;
/// Con voz activa se exige más tiempo de nivel constante antes de tomarlo por ruido (una vocal sostenida no cuenta).
const STAT_WINDOW_ACTIVE_S: f32 = 3.0;
const STATIONARY_RANGE_DB: f32 = 6.0;
/// Nivel máximo que debe tener el ruido de fondo tras el AGC cuando no hay voz.
const NOISE_OUT_MAX_DB: f32 = -52.0;

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

#[inline]
fn pow_to_db(p: f32) -> f32 {
    10.0 * p.max(1e-12).log10()
}

#[inline]
fn coef(dt: f32, tau: f32) -> f32 {
    1.0 - (-dt / tau).exp()
}

/// Potencia media (cuadrado medio) de un bloque entrelazado.
#[inline]
fn mean_square(block: &[f32]) -> f32 {
    if block.is_empty() {
        return 0.0;
    }
    let s: f32 = block.iter().map(|x| if x.is_finite() { x * x } else { 0.0 }).sum();
    s / block.len() as f32
}

// ───────────────────────────── detector de voz ─────────────────────────────

#[derive(Clone, Copy, Debug, Default)]
pub struct GateState {
    /// La señal activa varía como el habla (sílabas/pausas) y no es un nivel constante: ruido estacionario ⇒ false.
    pub speechy: bool,
    /// Hay voz ahora mismo (por encima del umbral, con histéresis).
    pub active: bool,
    /// `active` o dentro del tiempo de espera posterior a la voz.
    pub voiced: bool,
}

/// Detector de voz por nivel: compara la envolvente rápida con un piso de ruido que baja al instante
/// y sube despacio (o salta al nivel si el ruido es estacionario). No amplifica ni dispara con ruido constante.
pub struct VoiceGate {
    env: f32,
    floor_db: f32,
    elapsed: f32,
    active: bool,
    since_active: f32,
    stat_t: f32,
    stat_min: f32,
    stat_max: f32,
    speechy: bool,
    act_t: f32,
    act_min: f32,
    act_max: f32,
}

impl Default for VoiceGate {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceGate {
    pub fn new() -> Self {
        VoiceGate {
            env: 0.0,
            floor_db: FLOOR_MIN_DB,
            elapsed: 0.0,
            active: false,
            since_active: f32::MAX,
            stat_t: 0.0,
            stat_min: f32::MAX,
            stat_max: f32::MIN,
            speechy: false,
            act_t: 0.0,
            act_min: f32::MAX,
            act_max: f32::MIN,
        }
    }

    pub fn floor_db(&self) -> f32 {
        self.floor_db
    }

    /// `p` = potencia media del bloque; `dt` = duración del bloque en segundos.
    pub fn update(&mut self, p: f32, dt: f32) -> GateState {
        self.env += (p - self.env) * coef(dt, 0.02);
        let env_db = pow_to_db(self.env);
        self.elapsed += dt;
        if self.elapsed < WARMUP_S {
            // Aprende el nivel de fondo; la voz inicial se trata como fondo hasta la primera pausa.
            if self.elapsed > 0.05 {
                self.floor_db = self.floor_db.max(env_db);
            }
            return GateState::default();
        }
        // Piso: baja al instante, sube despacio.
        if env_db < self.floor_db {
            self.floor_db = env_db.max(FLOOR_MIN_DB);
        } else {
            self.floor_db += FLOOR_RISE_DB_PER_S * dt;
        }
        // Ruido estacionario (poca variación en la ventana): el piso salta a su nivel.
        self.stat_t += dt;
        self.stat_min = self.stat_min.min(env_db);
        self.stat_max = self.stat_max.max(env_db);
        if self.stat_t >= if self.active { STAT_WINDOW_ACTIVE_S } else { STAT_WINDOW_S } {
            if self.stat_max - self.stat_min < STATIONARY_RANGE_DB {
                self.floor_db = self.floor_db.max(self.stat_max);
            }
            self.stat_t = 0.0;
            self.stat_min = f32::MAX;
            self.stat_max = f32::MIN;
        }
        let open = (self.floor_db + OPEN_ABOVE_FLOOR_DB).max(ABS_OPEN_DB);
        let close = (self.floor_db + CLOSE_ABOVE_FLOOR_DB).max(ABS_CLOSE_DB);
        if self.active {
            if env_db < close {
                self.active = false;
            }
        } else if env_db > open {
            self.active = true;
        }
        if self.active {
            self.since_active = 0.0;
        } else {
            self.since_active += dt;
        }
        // «Habla» = el nivel activo varía ≥ 6 dB en 0,5 s de actividad. Se pierde tras 1 s sin voz.
        if self.active {
            self.act_t += dt;
            self.act_min = self.act_min.min(env_db);
            self.act_max = self.act_max.max(env_db);
            if self.act_t >= SPEECH_WINDOW_S {
                self.speechy = self.act_max - self.act_min >= STATIONARY_RANGE_DB;
                self.act_t = 0.0;
                self.act_min = f32::MAX;
                self.act_max = f32::MIN;
            }
        } else if self.since_active > SPEECH_FORGET_S {
            self.speechy = false;
            self.act_t = 0.0;
            self.act_min = f32::MAX;
            self.act_max = f32::MIN;
        }
        GateState { speechy: self.speechy, active: self.active, voiced: self.active || self.since_active < HANG_S }
    }
}

// ───────────────────────────── limitador look-ahead ─────────────────────────────

/// Limitador look-ahead de 2 ms (techo `CEILING`), con ganancia común a todos los canales.
/// La ganancia baja antes de que llegue el pico (mínimo deslizante + media móvil) y se recupera en ~80 ms.
pub struct Limiter {
    ch: usize,
    d: usize,
    delay: Vec<f32>,
    dpos: usize,
    mins: VecDeque<(u64, f32)>,
    idx: u64,
    avg: Vec<f32>,
    apos: usize,
    sum: f64,
    gm: f32,
    rel: f32,
}

impl Limiter {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let ch = channels.max(1) as usize;
        let d = ((sample_rate as f32 * 0.002).round() as usize).max(2);
        Limiter {
            ch,
            d,
            delay: vec![0.0; (d - 1) * ch],
            dpos: 0,
            mins: VecDeque::with_capacity(d + 4),
            idx: 0,
            avg: vec![1.0; d],
            apos: 0,
            sum: d as f64,
            gm: 1.0,
            rel: coef(1.0, sample_rate.max(1) as f32 * 0.08),
        }
    }

    /// Procesa en sitio un bloque entrelazado (múltiplo de `channels`). Latencia: `d - 1` cuadros.
    pub fn process(&mut self, buf: &mut [f32]) {
        let ch = self.ch;
        for frame in buf.chunks_exact_mut(ch) {
            let mut peak = 0.0f32;
            for x in frame.iter_mut() {
                if !x.is_finite() {
                    *x = 0.0;
                }
                peak = peak.max(x.abs());
            }
            let r = if peak > CEILING { CEILING / peak } else { 1.0 };
            while self.mins.back().is_some_and(|b| b.1 >= r) {
                self.mins.pop_back();
            }
            self.mins.push_back((self.idx, r));
            while self.mins.front().is_some_and(|f| f.0 + self.d as u64 <= self.idx) {
                self.mins.pop_front();
            }
            self.idx += 1;
            let m = self.mins.front().map_or(1.0, |f| f.1);
            self.gm = m.min(self.gm + (1.0 - self.gm) * self.rel);
            self.sum += (self.gm - self.avg[self.apos]) as f64;
            self.avg[self.apos] = self.gm;
            self.apos = (self.apos + 1) % self.d;
            let g = (self.sum / self.d as f64) as f32;
            // Retardo de d-1 cuadros: sale el más viejo, entra el nuevo.
            let slot = &mut self.delay[self.dpos * ch..(self.dpos + 1) * ch];
            for (x, s) in frame.iter_mut().zip(slot.iter_mut()) {
                let out = *s;
                *s = *x;
                *x = (out * g).clamp(-CEILING, CEILING);
            }
            self.dpos = (self.dpos + 1) % (self.d - 1);
        }
    }
}

// ───────────────────────────── procesador de entrada ─────────────────────────────

/// Cadena de una fuente: ganancia manual → AGC (opcional) → limitador.
pub struct VoiceProcessor {
    ch: usize,
    sr: f32,
    manual_prev: f32,
    gate: VoiceGate,
    lvl: f32,
    agc_db: f32,
    agc_prev: f32,
    limiter: Limiter,
}

impl VoiceProcessor {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        VoiceProcessor {
            ch: channels.max(1) as usize,
            sr: sample_rate.max(1) as f32,
            manual_prev: 1.0,
            gate: VoiceGate::new(),
            lvl: 0.0,
            agc_db: 0.0,
            agc_prev: 1.0,
            limiter: Limiter::new(sample_rate, channels),
        }
    }

    /// Ganancia actual del AGC en dB (diagnóstico).
    pub fn agc_gain_db(&self) -> f32 {
        self.agc_db
    }

    /// Procesa en sitio un bloque entrelazado (múltiplo de `channels`).
    pub fn process(&mut self, buf: &mut [f32], manual_gain: f32, agc_on: bool) {
        let manual = if manual_gain.is_finite() { manual_gain } else { 1.0 };
        let step = CTRL * self.ch;
        for block in buf.chunks_mut(step) {
            let frames = block.len() / self.ch;
            if frames == 0 {
                break;
            }
            ramp(block, self.ch, self.manual_prev, manual);
            self.manual_prev = manual;
            let target = if agc_on { self.agc_update(block, frames) } else { self.agc_idle() };
            ramp(block, self.ch, self.agc_prev, target);
            self.agc_prev = target;
        }
        self.limiter.process(buf);
    }

    fn agc_idle(&mut self) -> f32 {
        // AGC apagado: la ganancia vuelve a 0 dB (se interpola, sin saltos).
        self.agc_db = 0.0;
        1.0
    }

    fn agc_update(&mut self, block: &[f32], frames: usize) -> f32 {
        let dt = frames as f32 / self.sr;
        let p = mean_square(block);
        let st = self.gate.update(p, dt);
        // Nivel: ataque rápido, caída lenta (en potencia).
        let a = if p > self.lvl { coef(dt, 0.02) } else { coef(dt, 0.25) };
        self.lvl += (p - self.lvl) * a;
        let lvl_db = pow_to_db(self.lvl);
        let desired = (AGC_TARGET_DB + LEVEL_BIAS_DB - lvl_db).clamp(AGC_MIN_DB, AGC_MAX_DB);
        if desired < self.agc_db {
            // Señal fuerte: baja rápido (ataque ~10 ms).
            self.agc_db += (desired - self.agc_db) * coef(dt, 0.010);
        } else if st.active {
            // Solo sube con voz. Si está lejos del objetivo adquiere más rápido.
            let cap = if st.speechy { AGC_MAX_DB } else { UNPROVEN_MAX_DB.max(self.agc_db) };
            let err = desired.min(cap) - self.agc_db;
            let tau = if err > 6.0 { 0.12 } else { 0.4 };
            self.agc_db += err * coef(dt, tau);
        } else if !st.voiced {
            // Silencio: la ganancia se relaja hacia un valor que no deje el ruido de fondo por encima de -50 dBFS.
            let idle = (NOISE_OUT_MAX_DB - self.gate.floor_db()).clamp(0.0, AGC_MAX_DB);
            if self.agc_db > idle {
                self.agc_db = (self.agc_db - 6.0 * dt).max(idle);
            }
        }
        self.agc_db = self.agc_db.clamp(AGC_MIN_DB, AGC_MAX_DB);
        db_to_lin(self.agc_db)
    }
}

/// Multiplica el bloque por una ganancia interpolada linealmente de `g0` a `g1`.
fn ramp(block: &mut [f32], ch: usize, g0: f32, g1: f32) {
    if g0 == g1 {
        if g1 != 1.0 {
            block.iter_mut().for_each(|x| *x *= g1);
        }
        return;
    }
    let n = (block.len() / ch).max(1) as f32;
    for (i, frame) in block.chunks_exact_mut(ch).enumerate() {
        let g = g0 + (g1 - g0) * (i + 1) as f32 / n;
        frame.iter_mut().for_each(|x| *x *= g);
    }
}

// ───────────────────────────── ducking de la mezcla ─────────────────────────────

pub const DUCK_DB: f32 = 8.0;

/// Calcula una ganancia por cuadro (≤ 1) para el audio del sistema según la voz del micrófono.
pub struct Ducker {
    gate: VoiceGate,
    gain: f32,
    low: f32,
    sr: f32,
}

impl Ducker {
    pub fn new(sample_rate: u32) -> Self {
        Ducker { gate: VoiceGate::new(), gain: 1.0, low: db_to_lin(-DUCK_DB), sr: sample_rate.max(1) as f32 }
    }

    /// `voice`: audio entrelazado de `ch` canales del micrófono (cuadros). Rellena `out` con una ganancia por cuadro.
    pub fn gains(&mut self, voice: &[f32], ch: usize, out: &mut Vec<f32>) {
        out.clear();
        for block in voice.chunks(CTRL * ch) {
            let frames = block.len() / ch;
            if frames == 0 {
                break;
            }
            let dt = frames as f32 / self.sr;
            let st = self.gate.update(mean_square(block), dt);
            let (target, tau) = if st.voiced { (self.low, 0.05) } else { (1.0, 0.5) };
            let g0 = self.gain;
            let g1 = g0 + (target - g0) * coef(dt, tau);
            for i in 0..frames {
                out.push(g0 + (g1 - g0) * (i + 1) as f32 / frames as f32);
            }
            self.gain = g1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PRNG determinista (xorshift) → ruido blanco uniforme en [-1, 1].
    pub struct Rng(u64);
    impl Rng {
        pub fn new(seed: u64) -> Self {
            Rng(seed | 1)
        }
        pub fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 40) as f32 / (1u64 << 23) as f32) - 1.0
        }
    }

    fn rms_db(x: &[f32]) -> f32 {
        pow_to_db(mean_square(x))
    }

    /// Ruido «tipo voz»: gaussiano-ish a `db` dBFS RMS, con modulación silábica de 4 Hz.
    fn voice_like(rng: &mut Rng, frames: usize, sr: u32, db: f32) -> Vec<f32> {
        let a = db_to_lin(db) * 3f32.sqrt(); // uniforme: rms = a/√3
        (0..frames)
            .map(|i| {
                let t = i as f32 / sr as f32;
                let m = 0.75 + 0.25 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                // normaliza para que la media cuadrática de la modulación sea ~1
                rng.next() * a * m / 0.7603
            })
            .collect()
    }

    /// Mono, ráfagas de 0,35 s de voz y 0,25 s de pausa (ruido de fondo a `noise_db`).
    /// Devuelve la señal y una marca por muestra (true = ráfaga de voz).
    fn bursts(sr: u32, secs: f32, voice_db: f32, noise_db: f32) -> (Vec<f32>, Vec<bool>) {
        let mut rng = Rng::new(7);
        let n = (secs * sr as f32) as usize;
        let on = (0.35 * sr as f32) as usize;
        let period = (0.60 * sr as f32) as usize;
        let na = db_to_lin(noise_db) * 3f32.sqrt();
        let mut out = vec![0.0f32; n];
        let mut flags = vec![false; n];
        let mut k = 0;
        while k < n {
            let end = (k + on).min(n);
            out[k..end].copy_from_slice(&voice_like(&mut rng, end - k, sr, voice_db));
            flags[k..end].iter_mut().for_each(|f| *f = true);
            for x in &mut out[end..(k + period).min(n)] {
                *x = rng.next() * na;
            }
            k += period;
        }
        (out, flags)
    }

    fn run(proc_: &mut VoiceProcessor, sig: &[f32], manual: f32, agc: bool) -> Vec<f32> {
        let mut out = sig.to_vec();
        for chunk in out.chunks_mut(480) {
            proc_.process(chunk, manual, agc);
        }
        out
    }

    #[test]
    fn agc_brings_quiet_voice_to_target() {
        let sr = 48_000;
        let (sig, flags) = bursts(sr, 6.0, -42.0, -75.0);
        let mut p = VoiceProcessor::new(sr, 1);
        let out = run(&mut p, &sig, 1.0, true);
        // Tras 1 s de asentamiento: RMS de las ráfagas (sin la pausa).
        let from = sr as usize;
        let v: Vec<f32> = out[from..].iter().zip(&flags[from..]).filter(|(_, f)| **f).map(|(x, _)| *x).collect();
        let db = rms_db(&v);
        println!("AGC voz: salida {db:.1} dBFS RMS, ganancia {:.1} dB", p.agc_gain_db());
        assert!((-21.0..=-15.0).contains(&db), "RMS de voz tras AGC = {db:.1} dBFS");
    }

    #[test]
    fn agc_does_not_amplify_silence_or_stationary_noise() {
        let sr = 48_000;
        let mut rng = Rng::new(3);
        let a = db_to_lin(-60.0) * 3f32.sqrt();
        let sig: Vec<f32> = (0..sr as usize * 5).map(|_| rng.next() * a).collect();
        let mut p = VoiceProcessor::new(sr, 1);
        let out = run(&mut p, &sig, 1.0, true);
        let db = rms_db(&out[sr as usize..]);
        println!("AGC ruido -60: salida {db:.1} dBFS, ganancia {:.1} dB", p.agc_gain_db());
        assert!(db <= -50.0, "el ruido de fondo salió a {db:.1} dBFS");
    }

    #[test]
    fn agc_noise_appearing_after_digital_silence_is_not_boosted() {
        // 1 s casi mudo y luego ruido estacionario a -60 dBFS (p. ej. se enciende un ventilador).
        let sr = 48_000;
        let mut rng = Rng::new(21);
        let q = db_to_lin(-90.0) * 3f32.sqrt();
        let a = db_to_lin(-60.0) * 3f32.sqrt();
        let mut sig: Vec<f32> = (0..sr as usize).map(|_| rng.next() * q).collect();
        sig.extend((0..sr as usize * 5).map(|_| rng.next() * a));
        let mut p = VoiceProcessor::new(sr, 1);
        let out = run(&mut p, &sig, 1.0, true);
        let db = rms_db(&out[sr as usize * 4..]);
        println!("AGC ruido tras silencio: {db:.1} dBFS, ganancia {:.1} dB", p.agc_gain_db());
        assert!(db <= -50.0, "{db:.1} dBFS");
    }

    #[test]
    fn agc_noise_after_voice_relaxes() {
        // Voz (con ruido de fondo de -60 en las pausas) y luego solo ese ruido: la ganancia debe volver
        // para que el fondo quede ≤ -50 dBFS.
        let sr = 48_000;
        let (mut sig, _) = bursts(sr, 4.0, -42.0, -60.0);
        let mut rng = Rng::new(5);
        let a = db_to_lin(-60.0) * 3f32.sqrt();
        sig.extend((0..sr as usize * 8).map(|_| rng.next() * a));
        let mut p = VoiceProcessor::new(sr, 1);
        let out = run(&mut p, &sig, 1.0, true);
        let tail = &out[out.len() - sr as usize * 2..];
        let db = rms_db(tail);
        println!("AGC ruido tras voz: {db:.1} dBFS, ganancia {:.1} dB", p.agc_gain_db());
        assert!(db <= -50.0, "ruido tras la voz = {db:.1} dBFS");
    }

    #[test]
    fn limiter_holds_ceiling_with_hot_input() {
        let sr = 48_000;
        let mut rng = Rng::new(11);
        // +20 dB sobre fondo de escala: senos y ruido con picos enormes, estéreo.
        let mut sig = Vec::new();
        for i in 0..sr as usize * 2 {
            let t = i as f32 / sr as f32;
            let s = 10.0 * (2.0 * std::f32::consts::PI * 220.0 * t).sin() + rng.next() * 3.0;
            sig.push(s);
            sig.push(-s * 0.8);
        }
        sig[5000] = 25.0;
        sig[5001] = f32::NAN;
        let mut lim = Limiter::new(sr, 2);
        for chunk in sig.chunks_mut(960) {
            lim.process(chunk);
        }
        let worst = sig.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        println!("limitador: pico de salida {worst:.4} ({:.2} dBFS)", lin_to_db(worst));
        assert!(worst <= 0.9, "pico {worst}");
        assert!(sig.iter().all(|x| x.is_finite()));
        // Transparente por debajo del techo: un seno a -6 dBFS casi no cambia (salvo el retardo).
        let mut lim = Limiter::new(sr, 1);
        let mut quiet: Vec<f32> =
            (0..sr as usize).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin()).collect();
        let orig = quiet.clone();
        lim.process(&mut quiet);
        let d = 95; // latencia
        let err = quiet[d + 100..].iter().zip(&orig[100..]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(err < 1e-4, "el limitador alteró una señal limpia: {err}");
    }

    #[test]
    fn manual_gain_and_agc_off_pass_through_with_limiter() {
        let sr = 48_000;
        let mut p = VoiceProcessor::new(sr, 2);
        let mut buf: Vec<f32> = (0..9600).map(|i| 0.1 * ((i / 2) as f32 * 0.05).sin()).collect();
        let orig = buf.clone();
        for c in buf.chunks_mut(960) {
            p.process(c, 2.0, false);
        }
        // +6 dB aprox; (retardo de 95 cuadros)
        let lat = 95 * 2;
        let ratio = buf[lat + 2000] / orig[2000];
        assert!((ratio - 2.0).abs() < 0.01, "ratio {ratio}");
    }

    #[test]
    fn ducker_lowers_system_during_voice_and_recovers() {
        let sr = 48_000u32;
        let mut rng = Rng::new(9);
        // 1 s de silencio, 2 s de voz, 3 s de silencio.
        let mut voice = vec![0.0f32; sr as usize];
        voice.extend(voice_like(&mut rng, sr as usize * 2, sr, -25.0));
        voice.extend(vec![0.0f32; sr as usize * 3]);
        let mut d = Ducker::new(sr);
        let mut g = Vec::new();
        d.gains(&voice, 1, &mut g);
        assert_eq!(g.len(), voice.len());
        let at = |s: f32| g[(s * sr as f32) as usize];
        let during = lin_to_db(at(2.5));
        let before = lin_to_db(at(0.9));
        let after = lin_to_db(at(5.9));
        println!("ducking: antes {before:.2} dB, durante {during:.2} dB, después {after:.2} dB");
        assert!(before > -0.5);
        assert!(during <= -6.0, "durante la voz {during:.2} dB");
        assert!(after > -1.0, "no recupera: {after:.2} dB");
    }
}
