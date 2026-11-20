//! Motor: estado, streams, hilo escritor e hilo de eventos.
//!
//! Hilos:
//! - **control**: dueño de los `cpal::Stream` (no son `Send`); recibe órdenes por canal.
//! - **escritor** (uno por grabación): vacía los ring buffers a WAV con `hound` y rellena huecos por reloj.
//! - **eventos**: `levels` ~30 Hz y `state` cada ~250 ms grabando.
//! - **sondeo de dispositivos**: cada ~3 s → `devices-changed` si cambió la lista.
//!
//! Los callbacks de audio solo escriben en atómicos y en el ring (ver `capture`).

use super::capture::{open_microphone, ErrorSink, Opened, SourceShared, UNSET};
use super::devices;
use super::loopback::{open_loopback, silence_deficit};
use super::meters::MeterReader;
use super::mixer::{mix_files, MixInput, TrackRole};
use super::wav::TrackWriter;
use crate::library::{self, SessionJson};
use crate::types::*;
use ringbuf::traits::{Consumer, Observer};
use ringbuf::HeapCons;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::*};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum EngineEvent {
    Levels(Levels),
    State(EngineState),
    RecordingSaved(Recording),
    DevicesChanged(Devices),
    Error(EngineErrorPayload),
}

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Input,
    Output,
}

impl Role {
    const ALL: [Role; 2] = [Role::Input, Role::Output];
    fn idx(self) -> usize {
        self as usize
    }
    fn label(self) -> &'static str {
        match self {
            Role::Input => "el micrófono",
            Role::Output => "el audio del sistema",
        }
    }
    fn file(self) -> &'static str {
        match self {
            Role::Input => library::FILE_INPUT,
            Role::Output => library::FILE_OUTPUT,
        }
    }
}

/// Descripción de un stream abierto (para diagnóstico / sonda).
#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub keepalive: bool,
}

fn lk<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────────── estado compartido ─────────────────────────────

struct RunClock {
    accumulated: Duration,
    since: Option<Instant>,
}

impl RunClock {
    fn new() -> Self {
        RunClock { accumulated: Duration::ZERO, since: None }
    }
    fn restart(&mut self) {
        self.accumulated = Duration::ZERO;
        self.since = Some(Instant::now());
    }
    fn pause(&mut self) {
        if let Some(s) = self.since.take() {
            self.accumulated += s.elapsed();
        }
    }
    fn resume(&mut self) {
        if self.since.is_none() {
            self.since = Some(Instant::now());
        }
    }
    fn elapsed(&self) -> Duration {
        self.accumulated + self.since.map(|s| s.elapsed()).unwrap_or_default()
    }
}

struct StatusInner {
    status: Status,
    recording_id: Option<String>,
}

struct Core {
    settings: Mutex<Settings>,
    persist: bool,
    status: Mutex<StatusInner>,
    clock: Mutex<RunClock>,
    sources: [Arc<SourceShared>; 2],
    sink: EventSink,
    last_devices: Mutex<Devices>,
    shutdown: AtomicBool,
}

impl Core {
    fn emit(&self, e: EngineEvent) {
        (self.sink)(e);
    }
    fn error(&self, message: impl Into<String>, fatal: bool) {
        self.emit(EngineEvent::Error(EngineErrorPayload { message: message.into(), fatal }));
    }
    fn state(&self) -> EngineState {
        let (status, recording_id) = {
            let s = lk(&self.status);
            (s.status, s.recording_id.clone())
        };
        let elapsed_ms = match status {
            Status::Idle => 0,
            _ => lk(&self.clock).elapsed().as_millis() as u64,
        };
        EngineState { status, elapsed_ms, recording_id }
    }
    fn emit_state(&self) {
        self.emit(EngineEvent::State(self.state()));
    }
    fn status(&self) -> Status {
        lk(&self.status).status
    }
    fn set_status(&self, status: Status, id: Option<String>) {
        let mut s = lk(&self.status);
        s.status = status;
        s.recording_id = id;
    }
}

// ───────────────────────────── API pública ─────────────────────────────

enum Ctl {
    Configure { reply: Sender<()> },
    Start { settings: Option<Settings>, reply: Sender<Result<EngineState, String>> },
    Pause { reply: Sender<Result<EngineState, String>> },
    Resume { reply: Sender<Result<EngineState, String>> },
    Stop { reply: Sender<Result<Finished, String>> },
    Info { reply: Sender<[Option<StreamInfo>; 2]> },
    StreamError { role: Role, gen: u64, message: String },
    WriterFailed(String),
    Simulate { role: Role },
    DevicesChanged,
    Shutdown,
}

/// Pasa ganancias (dB → lineal) y AGC de los ajustes a las fuentes; el AGC solo aplica al micrófono.
fn apply_gains(sources: &[Arc<SourceShared>; 2], s: &Settings) {
    sources[0].set_gain(super::dsp::db_to_lin(s.input_gain_db));
    sources[1].set_gain(super::dsp::db_to_lin(s.output_gain_db));
    sources[0].set_agc(s.input_agc);
    sources[1].set_agc(false);
}

#[derive(Clone)]
pub struct Engine {
    core: Arc<Core>,
    ctl: Sender<Ctl>,
}

const GONE: &str = "El motor de audio no responde.";

impl Engine {
    /// Crea el motor y arranca sus hilos. Si `persist` es true, `set_settings` escribe en `%APPDATA%\Eco`.
    pub fn new(settings: Settings, sink: EventSink, persist: bool) -> Engine {
        let settings = settings.validated();
        let epoch = Instant::now();
        let sources = [Arc::new(SourceShared::new(epoch)), Arc::new(SourceShared::new(epoch))];
        apply_gains(&sources, &settings);
        let core = Arc::new(Core {
            settings: Mutex::new(settings),
            persist,
            status: Mutex::new(StatusInner { status: Status::Idle, recording_id: None }),
            clock: Mutex::new(RunClock::new()),
            sources,
            sink,
            last_devices: Mutex::new(devices::list_devices()),
            shutdown: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::channel::<Ctl>();

        {
            let (core, tx) = (core.clone(), tx.clone());
            thread::Builder::new()
                .name("eco-control".into())
                .spawn(move || Control::new(core, tx).run(rx))
                .expect("hilo de control");
        }
        {
            let core = core.clone();
            thread::Builder::new()
                .name("eco-events".into())
                .spawn(move || events_main(core))
                .expect("hilo de eventos");
        }
        {
            let (core, tx) = (core.clone(), tx.clone());
            thread::Builder::new()
                .name("eco-devices".into())
                .spawn(move || devices_poll_main(core, tx))
                .expect("hilo de dispositivos");
        }
        let eng = Engine { core, ctl: tx };
        // Abre los streams de monitoreo iniciales (sin bloquear al llamador).
        let (rtx, _rrx) = mpsc::channel();
        let _ = eng.ctl.send(Ctl::Configure { reply: rtx });
        eng
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Ctl) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.ctl.send(make(tx)).map_err(|_| GONE.to_string())?;
        rx.recv().map_err(|_| GONE.to_string())
    }

    pub fn list_devices(&self) -> Devices {
        devices::list_devices()
    }

    pub fn settings(&self) -> Settings {
        lk(&self.core.settings).clone()
    }

    pub fn state(&self) -> EngineState {
        self.core.state()
    }

    /// Valida, persiste y aplica. Reabre streams si cambian dispositivos/fuentes/monitor.
    pub fn set_settings(&self, new: Settings) -> Result<Settings, String> {
        let new = new.validated();
        new.check_output_dir()?;
        let old = self.settings();
        let sources_changed = old.input_id != new.input_id
            || old.output_id != new.output_id
            || old.capture_input != new.capture_input
            || old.capture_output != new.capture_output;
        if sources_changed && self.core.status() != Status::Idle {
            return Err("No se pueden cambiar los dispositivos ni las fuentes mientras se graba.".into());
        }
        if self.core.persist {
            crate::settings::save(&new)?;
        }
        *lk(&self.core.settings) = new.clone();
        apply_gains(&self.core.sources, &new);
        if sources_changed || old.monitor_when_idle != new.monitor_when_idle {
            let _ = self.ask(|reply| Ctl::Configure { reply });
        }
        Ok(new)
    }

    pub fn start_recording(&self) -> Result<EngineState, String> {
        self.ask(|reply| Ctl::Start { settings: None, reply })?
    }

    /// Commit the UI snapshot and start on the same control-thread turn.
    /// Never start with stale WAV/source switches while an async UI save is pending.
    pub fn start_recording_with_settings(&self, settings: Settings) -> Result<EngineState, String> {
        self.ask(|reply| Ctl::Start { settings: Some(settings), reply })?
    }
    pub fn pause_recording(&self) -> Result<EngineState, String> {
        self.ask(|reply| Ctl::Pause { reply })?
    }
    pub fn resume_recording(&self) -> Result<EngineState, String> {
        self.ask(|reply| Ctl::Resume { reply })?
    }

    /// Detiene, cierra las pistas y genera la mezcla si aplica. Bloquea hasta terminar.
    pub fn stop_recording(&self) -> Result<Recording, String> {
        let fin = self.ask(|reply| Ctl::Stop { reply })??;
        finalize_session(&self.core, fin)
    }

    /// Formato nativo de los streams abiertos `[micrófono, loopback]`.
    pub fn stream_info(&self) -> [Option<StreamInfo>; 2] {
        self.ask(|reply| Ctl::Info { reply }).unwrap_or([None, None])
    }

    /// Solo pruebas: simula la desconexión del dispositivo (`input` = micrófono, si no loopback),
    /// con el mismo camino que un error real de stream.
    #[doc(hidden)]
    pub fn simulate_device_loss(&self, input: bool) {
        let role = if input { Role::Input } else { Role::Output };
        let _ = self.ctl.send(Ctl::Simulate { role });
    }

    pub fn list_recordings(&self) -> Vec<Recording> {
        let active = self.state().recording_id;
        library::list_recordings(&self.settings().output_dir).into_iter()
            .filter(|r| active.as_deref() != Some(r.id.as_str())).collect()
    }
    pub fn rename_recording(&self, id: &str, name: &str) -> Result<Recording, String> {
        if self.state().recording_id.as_deref() == Some(id) { return Err("Нельзя переименовать текущую запись до остановки.".into()); }
        library::rename_recording(&self.settings().output_dir, id, name)
    }
    pub fn delete_recording(&self, id: &str) -> Result<(), String> {
        if self.state().recording_id.as_deref() == Some(id) { return Err("Нельзя удалить текущую запись до остановки.".into()); }
        library::delete_recording(&self.settings().output_dir, id)
    }

    /// Cierre ordenado: si hay una grabación en curso la detiene y la guarda.
    pub fn shutdown(&self) {
        if matches!(self.core.status(), Status::Recording | Status::Paused) {
            let _ = self.stop_recording();
        }
        self.core.shutdown.store(true, Release);
        let _ = self.ctl.send(Ctl::Shutdown);
    }
}

// ───────────────────────────── hilo de control ─────────────────────────────

struct Slot {
    setting_id: String,
    gen: u64,
    opened: Opened,
    /// `None` mientras lo tiene el hilo escritor.
    consumer: Option<HeapCons<f32>>,
}

struct TrackMeta {
    role: Role,
    path: PathBuf,
}

struct Session {
    id: String,
    name: String,
    created_at: String,
    dir: PathBuf,
    settings: Settings,
    tracks: Vec<TrackMeta>,
    writer: Option<JoinHandle<Vec<TrackResult>>>,
    stop: Arc<AtomicBool>,
}

struct TrackResult {
    role: Role,
    cons: HeapCons<f32>,
    offset_ns: u64,
    error: Option<String>,
}

struct FinTrack {
    role: Role,
    path: PathBuf,
    offset_ns: u64,
}

/// Sesión ya detenida y con pistas cerradas, pendiente de mezcla.
pub struct Finished {
    id: String,
    name: String,
    created_at: String,
    dir: PathBuf,
    settings: Settings,
    duration_ms: u64,
    tracks: Vec<FinTrack>,
    write_failed: bool,
}

struct Control {
    core: Arc<Core>,
    tx: Sender<Ctl>,
    slots: [Option<Slot>; 2],
    next_gen: u64,
    last_open_err: [Option<String>; 2],
    session: Option<Session>,
    #[cfg(test)]
    synthetic_sources: bool,
}

impl Control {
    fn new(core: Arc<Core>, tx: Sender<Ctl>) -> Self {
        Control { core, tx, slots: [None, None], next_gen: 1, last_open_err: [None, None], session: None,
            #[cfg(test)] synthetic_sources: false }
    }

    fn run(mut self, rx: Receiver<Ctl>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                Ctl::Shutdown => break,
                Ctl::Configure { reply } => {
                    self.monitor_reconcile();
                    let _ = reply.send(());
                }
                Ctl::DevicesChanged => self.monitor_reconcile(),
                Ctl::Start { settings, reply } => {
                    let result = self.apply_start_settings(settings).and_then(|_| self.start());
                    let _ = reply.send(result);
                }
                Ctl::Pause { reply } => {
                    let _ = reply.send(self.pause());
                }
                Ctl::Resume { reply } => {
                    let _ = reply.send(self.resume());
                }
                Ctl::Stop { reply } => {
                    let _ = reply.send(self.do_stop());
                }
                Ctl::Info { reply } => {
                    let mut out = [None, None];
                    for r in Role::ALL {
                        out[r.idx()] = self.slots[r.idx()].as_ref().map(|s| StreamInfo {
                            device_name: s.opened.device_name.clone(),
                            sample_rate: s.opened.sample_rate,
                            channels: s.opened.channels,
                            keepalive: s.opened.keepalive,
                        });
                    }
                    let _ = reply.send(out);
                }
                Ctl::StreamError { role, gen, message } => self.stream_error(role, gen, message),
                Ctl::Simulate { role } => {
                    if let Some(gen) = self.slots[role.idx()].as_ref().map(|s| s.gen) {
                        self.stream_error(role, gen, "simulado".into());
                    }
                }
                Ctl::WriterFailed(message) => {
                    if self.session.is_some() {
                        self.fail_session(format!(
                            "Ошибка записи на диск ({message}). Запись остановлена. Полученные файлы оставлены; полнота и воспроизводимость не гарантируются."
                        ));
                    }
                }
            }
        }
        for r in Role::ALL {
            self.close(r);
        }
    }

    fn close(&mut self, role: Role) {
        self.slots[role.idx()] = None;
        self.core.sources[role.idx()].open.store(false, Release);
    }

    fn open(&mut self, role: Role, id: &str) -> Result<(), String> {
        let shared = self.core.sources[role.idx()].clone();
        let gen = self.next_gen;
        self.next_gen += 1;
        let tx = self.tx.clone();
        let on_err: ErrorSink = Arc::new(move |message| {
            let _ = tx.send(Ctl::StreamError { role, gen, message });
        });
        let mut opened = match role {
            Role::Input => open_microphone(id, shared.clone(), on_err)?,
            Role::Output => open_loopback(id, shared.clone(), on_err)?,
        };
        let consumer = opened.consumer.take();
        self.slots[role.idx()] = Some(Slot { setting_id: id.to_string(), gen, opened, consumer });
        shared.meter.take();
        shared.open.store(true, Release);
        Ok(())
    }

    /// Deja abiertos exactamente los streams que se necesitan. Devuelve errores por fuente.
    fn reconcile(&mut self, want: [bool; 2], s: &Settings) -> Vec<(Role, String)> {
        #[cfg(test)]
        if self.synthetic_sources {
            return Role::ALL.into_iter().filter(|r|want[r.idx()] && self.slots[r.idx()].is_none())
                .map(|r|(r, "Synthetic source not supplied".into())).collect();
        }
        let mut errs = Vec::new();
        for role in Role::ALL {
            let id = match role {
                Role::Input => s.input_id.as_str(),
                Role::Output => s.output_id.as_str(),
            };
            if !want[role.idx()] {
                self.close(role);
                self.last_open_err[role.idx()] = None;
                continue;
            }
            let up_to_date = self.slots[role.idx()].as_ref().is_some_and(|sl| {
                sl.setting_id == id
                    && devices::resolved_name(id, role == Role::Input).as_deref()
                        == Some(sl.opened.device_name.as_str())
            });
            if up_to_date {
                continue;
            }
            self.close(role);
            match self.open(role, id) {
                Ok(()) => self.last_open_err[role.idx()] = None,
                Err(e) => errs.push((role, e)),
            }
        }
        errs
    }

    /// Monitoreo en reposo: abre/cierra streams según ajustes (solo si está inactivo).
    fn monitor_reconcile(&mut self) {
        if self.session.is_some() || self.core.status() != Status::Idle {
            return;
        }
        let s = lk(&self.core.settings).clone();
        let want = [s.capture_input && s.monitor_when_idle, s.capture_output && s.monitor_when_idle];
        for (role, e) in self.reconcile(want, &s) {
            // Evita repetir el mismo aviso en cada sondeo.
            if self.last_open_err[role.idx()].as_deref() != Some(e.as_str()) {
                self.core.error(e.clone(), false);
                self.last_open_err[role.idx()] = Some(e);
            }
        }
    }

    fn apply_start_settings(&mut self, settings: Option<Settings>) -> Result<(), String> {
        let Some(settings) = settings else { return Ok(()); };
        if self.core.status() != Status::Idle { return Err("Сначала остановите текущую запись.".into()); }
        let settings = settings.validated();
        settings.check_output_dir()?;
        if self.core.persist { crate::settings::save(&settings)?; }
        apply_gains(&self.core.sources, &settings);
        *lk(&self.core.settings) = settings;
        Ok(())
    }

    fn start(&mut self) -> Result<EngineState, String> {
        match self.core.status() {
            Status::Idle => {}
            Status::Finalizing => return Err("Espera un momento: se está guardando la grabación anterior.".into()),
            _ => return Err("Ya hay una grabación en curso.".into()),
        }
        let s = lk(&self.core.settings).clone();
        if !s.capture_input && !s.capture_output {
            return Err("No hay ninguna fuente activa. Activa el micrófono o el audio del sistema.".into());
        }
        let want = [s.capture_input, s.capture_output];
        let errs = self.reconcile(want, &s);
        if let Some((role, e)) = errs.into_iter().next() {
            let _ = role;
            self.monitor_reconcile();
            return Err(e);
        }
        let started = self.start_session(&s, want);
        if started.is_err() {
            self.monitor_reconcile();
        }
        started
    }

    fn start_session(&mut self, s: &Settings, want: [bool; 2]) -> Result<EngineState, String> {
        let both = want[0] && want[1];
        let direct = s.direct_mp3();
        debug_assert_eq!(want, [s.capture_input, s.capture_output]);
        let mut reserved = None;
        let ns = if direct {
            let kind = if want[0] { TrackKind::Input } else { TrackKind::Output };
            let role = if want[0] { Role::Input } else { Role::Output };
            let o = &self.slots[role.idx()].as_ref().ok_or("La fuente no está disponible.")?.opened;
            let (rec, file) = library::reserve_direct_mp3(std::path::Path::new(&s.output_dir), kind, o.sample_rate, o.channels)?;
            reserved = Some((PathBuf::from(&rec.files[0].path), file));
            library::NewSession { id: rec.id, name: rec.name, created_at: rec.created_at, dir: PathBuf::from(&s.output_dir) }
        } else { library::create_session_dir(&s.output_dir)? };
        // En «solo mezcla» las pistas son temporales: float 32 evita perder precisión antes de mezclar.
        let track_fmt = if s.mode == Mode::Mixed && both { Format::Wav32f } else { s.format };

        let mut writers: Vec<(Role, TrackWriter)> = Vec::new();
        let mut metas: Vec<TrackMeta> = Vec::new();
        for role in Role::ALL.into_iter().filter(|r| want[r.idx()]) {
            let o = &self.slots[role.idx()].as_ref().ok_or("La fuente no está disponible.")?.opened;
            let (path, writer) = if let Some((path, file)) = reserved.take() {
                (path, TrackWriter::direct_mp3(file, o.sample_rate, o.channels))
            } else {
                let path = ns.dir.join(role.file());
                let writer = TrackWriter::create(&path, o.sample_rate, o.channels, track_fmt);
                (path, writer)
            };
            match writer {
                Ok(w) => writers.push((role, w)),
                Err(e) => {
                    drop(writers);
                    if !direct {
                        for meta in &metas { let _ = std::fs::remove_file(&meta.path); }
                        let _ = std::fs::remove_dir(&ns.dir);
                    }
                    return Err(e);
                }
            }
            metas.push(TrackMeta { role, path });
        }

        let mut wtracks = Vec::new();
        for (role, writer) in writers {
            let slot = self.slots[role.idx()].as_mut().expect("slot");
            let Some(mut cons) = slot.consumer.take() else {
                return Err("La fuente de audio sigue ocupada. Inténtalo de nuevo.".into());
            };
            cons.clear();
            wtracks.push(WTrack {
                role,
                cons,
                writer,
                sample_rate: slot.opened.sample_rate,
                channels: slot.opened.channels,
                shared: self.core.sources[role.idx()].clone(),
                offset_ns: None,
                failed: None,
            });
        }

        let stop = Arc::new(AtomicBool::new(false));
        {
            self.core.clock.lock().unwrap_or_else(|e| e.into_inner()).restart();
        }
        for t in &wtracks {
            let sh = &t.shared;
            sh.first_frame_ns.store(UNSET, Release);
            sh.rec_start_ns.store(sh.now_ns(), Release);
            sh.dropped.store(0, Relaxed);
            sh.paused.store(false, Release);
        }
        let handle = {
            let (core, stop, tx) = (self.core.clone(), stop.clone(), self.tx.clone());
            thread::Builder::new()
                .name("eco-writer".into())
                .spawn(move || writer_main(core, wtracks, stop, tx))
                .map_err(|e| format!("No se pudo iniciar el hilo de escritura: {e}"))?
        };
        for m in &metas {
            self.core.sources[m.role.idx()].recording.store(true, Release);
        }
        self.session = Some(Session {
            id: ns.id.clone(),
            name: ns.name,
            created_at: ns.created_at,
            dir: ns.dir,
            settings: s.clone(),
            tracks: metas,
            writer: Some(handle),
            stop,
        });
        self.core.set_status(Status::Recording, Some(ns.id));
        let st = self.core.state();
        self.core.emit(EngineEvent::State(st.clone()));
        Ok(st)
    }

    fn pause(&mut self) -> Result<EngineState, String> {
        let Some(sess) = self.session.as_ref() else {
            return Err("No hay una grabación en curso.".into());
        };
        match self.core.status() {
            Status::Paused => return Ok(self.core.state()),
            Status::Recording => {}
            _ => return Err("No hay una grabación en curso.".into()),
        }
        lk(&self.core.clock).pause();
        for t in &sess.tracks {
            self.core.sources[t.role.idx()].paused.store(true, Release);
        }
        let id = lk(&self.core.status).recording_id.clone();
        self.core.set_status(Status::Paused, id);
        let st = self.core.state();
        self.core.emit(EngineEvent::State(st.clone()));
        Ok(st)
    }

    fn resume(&mut self) -> Result<EngineState, String> {
        let Some(sess) = self.session.as_ref() else {
            return Err("No hay una grabación en curso.".into());
        };
        match self.core.status() {
            Status::Recording => return Ok(self.core.state()),
            Status::Paused => {}
            _ => return Err("No hay una grabación en curso.".into()),
        }
        lk(&self.core.clock).resume();
        for t in &sess.tracks {
            self.core.sources[t.role.idx()].paused.store(false, Release);
        }
        let id = lk(&self.core.status).recording_id.clone();
        self.core.set_status(Status::Recording, id);
        let st = self.core.state();
        self.core.emit(EngineEvent::State(st.clone()));
        Ok(st)
    }

    /// Congela el reloj, detiene los callbacks, drena y cierra las pistas.
    fn do_stop(&mut self) -> Result<Finished, String> {
        let mut sess = self.session.take().ok_or("No hay una grabación en curso.")?;
        let id = lk(&self.core.status).recording_id.clone();
        self.core.set_status(Status::Finalizing, id);
        let duration = {
            let mut c = lk(&self.core.clock);
            c.pause();
            c.elapsed()
        };
        self.core.emit_state();
        for t in &sess.tracks {
            let sh = &self.core.sources[t.role.idx()];
            sh.recording.store(false, Release);
            sh.paused.store(false, Release);
        }
        // Deja terminar los callbacks en vuelo antes de la última vaciada.
        thread::sleep(Duration::from_millis(40));
        sess.stop.store(true, Release);
        let results = match sess.writer.take().map(|h| h.join()) {
            Some(Ok(r)) => r,
            _ => {
                self.core.set_status(Status::Idle, None);
                self.core.emit_state();
                return Err("Falló el hilo de escritura del audio.".into());
            }
        };
        let mut tracks = Vec::new();
        let mut write_failed = false;
        for r in results {
            if let Some(slot) = self.slots[r.role.idx()].as_mut() {
                slot.consumer = Some(r.cons);
            }
            if let Some(e) = r.error {
                write_failed = true;
                self.core.error(format!("Ошибка записи {}: {e}. Файл оставлен: {}. Полнота и воспроизводимость не проверены.", r.role.label(), sess.dir.display()), false);
            }
            let meta = sess.tracks.iter().find(|m| m.role == r.role).expect("meta");
            tracks.push(FinTrack { role: r.role, path: meta.path.clone(), offset_ns: r.offset_ns });
        }
        // Sin monitoreo en reposo, se sueltan los dispositivos al terminar.
        if !lk(&self.core.settings).monitor_when_idle {
            for r in Role::ALL {
                self.close(r);
            }
        }
        Ok(Finished {
            id: sess.id,
            name: sess.name,
            created_at: sess.created_at,
            dir: sess.dir,
            settings: sess.settings,
            duration_ms: duration.as_millis() as u64,
            tracks,
            write_failed,
        })
    }

    /// Error fatal durante la grabación: detiene, guarda lo grabado y avisa.
    fn fail_session(&mut self, message: String) {
        match self.do_stop() {
            Ok(fin) => {
                let core = self.core.clone();
                let _ = thread::Builder::new().name("eco-finalize".into()).spawn(move || {
                    core.error(message, true);
                    if let Err(e) = finalize_session(&core, fin) {
                        core.error(e, true);
                    }
                });
            }
            Err(e) => self.core.error(format!("{message} {e}"), true),
        }
    }

    fn stream_error(&mut self, role: Role, gen: u64, _detail: String) {
        if self.slots[role.idx()].as_ref().map(|s| s.gen) != Some(gen) {
            return; // evento de un stream ya cerrado
        }
        let device = self.slots[role.idx()]
            .as_ref()
            .map(|s| s.opened.device_name.clone())
            .unwrap_or_default();
        if self.session.is_some() {
            let msg = format!(
                "Потерян источник {} («{device}»). Запись остановлена; выполняется финализация полученных данных.",
                role.label()
            );
            self.fail_session(msg);
            self.close(role);
        } else {
            self.close(role);
            self.core.error(
                format!("Se desconectó {} («{device}»). Se reconectará cuando vuelva a estar disponible.", role.label()),
                false,
            );
        }
    }
}

// ───────────────────────────── hilo escritor ─────────────────────────────

struct WTrack {
    role: Role,
    cons: HeapCons<f32>,
    writer: TrackWriter,
    sample_rate: u32,
    channels: u16,
    shared: Arc<SourceShared>,
    offset_ns: Option<u64>,
    failed: Option<String>,
}

/// Cuánto atraso respecto al reloj se tolera antes de rellenar con silencio (jitter normal de buffers).
const FILL_THRESHOLD_MS: u64 = 250;

fn writer_main(core: Arc<Core>, mut tracks: Vec<WTrack>, stop: Arc<AtomicBool>, ctl: Sender<Ctl>) -> Vec<TrackResult> {
    let mut buf = vec![0.0f32; 16_384];
    let mut last_flush = Instant::now();
    loop {
        let stopping = stop.load(Acquire);
        let mut moved = false;
        for t in tracks.iter_mut() {
            let ch = t.channels.max(1) as usize;
            let chunk = (buf.len() / ch) * ch;
            loop {
                let n = (t.cons.occupied_len().min(chunk) / ch) * ch;
                if n == 0 {
                    break;
                }
                let got = t.cons.pop_slice(&mut buf[..n]);
                moved = true;
                if t.failed.is_some() {
                    continue;
                }
                if t.offset_ns.is_none() && t.writer.frames == 0 {
                    let first = t.shared.first_frame_ns.load(Acquire);
                    let start = t.shared.rec_start_ns.load(Acquire);
                    t.offset_ns = Some(if first == UNSET { 0 } else { first.saturating_sub(start) });
                }
                if let Err(e) = t.writer.write(&buf[..got]) {
                    t.failed = Some(e.clone());
                    let _ = ctl.send(Ctl::WriterFailed(e));
                }
            }
        }
        // Relleno por reloj: el loopback no entrega nada si no suena nada (y cualquier hueco real).
        let elapsed = lk(&core.clock).elapsed().as_secs_f64();
        for t in tracks.iter_mut().filter(|t| t.failed.is_none()) {
            let sr = t.sample_rate as u64;
            let expected = (elapsed * sr as f64).round() as u64;
            let fill = silence_deficit(expected, t.writer.frames, sr * FILL_THRESHOLD_MS / 1000, stopping);
            if fill > 0 {
                if t.offset_ns.is_none() && t.writer.frames == 0 {
                    t.offset_ns = Some(0);
                }
                moved = true;
                if let Err(e) = t.writer.write_silence(fill) {
                    t.failed = Some(e.clone());
                    let _ = ctl.send(Ctl::WriterFailed(e));
                }
            }
        }
        if stopping {
            break;
        }
        if last_flush.elapsed() >= Duration::from_secs(1) {
            // Mantiene la cabecera WAV al día: el archivo es válido aunque se corte el proceso.
            for t in tracks.iter_mut().filter(|t| t.failed.is_none()) {
                if let Err(e) = t.writer.flush() {
                    t.failed = Some(e.clone());
                    let _ = ctl.send(Ctl::WriterFailed(e));
                }
            }
            last_flush = Instant::now();
        }
        if !moved {
            thread::sleep(Duration::from_millis(5));
        }
    }
    tracks
        .into_iter()
        .map(|t| {
            let mut error = t.failed;
            if let Err(e) = t.writer.finalize() {
                error.get_or_insert(e);
            }
            TrackResult { role: t.role, cons: t.cons, offset_ns: t.offset_ns.unwrap_or(0), error }
        })
        .collect()
}

// ───────────────────────────── finalización ─────────────────────────────

fn finalize_session(core: &Arc<Core>, fin: Finished) -> Result<Recording, String> {
    let result = finalize_inner(core, &fin);
    core.set_status(Status::Idle, None);
    core.emit_state();
    if let Ok(rec) = &result {
        core.emit(EngineEvent::RecordingSaved(rec.clone()));
    }
    result
}

fn finalize_inner(core: &Arc<Core>, fin: &Finished) -> Result<Recording, String> {
    if fin.settings.format == Format::Mp3 && fin.tracks.len() == 1 {
        let track = &fin.tracks[0];
        let kind = if track.role == Role::Input { TrackKind::Input } else { TrackKind::Output };
        let rec = library::finish_direct_mp3(&fin.dir, &fin.id, &track.path, kind, fin.duration_ms)?;
        if fin.write_failed {
            return Err(format!("Ошибка записи/финализации MP3. Полученный файл оставлен: {}. Восстановление и полнота не подтверждены.", track.path.display()));
        }
        return Ok(rec);
    }
    let mode = fin.settings.mode;
    let mut mix_ok = false;
    if fin.tracks.len() == 2 && matches!(mode, Mode::Mixed | Mode::Both) {
        let out = fin.dir.join(library::FILE_MIX);
        // Las ganancias ya están aplicadas en cada pista al grabar (ganancia 1.0 aquí).
        let inputs: Vec<MixInput> = fin
            .tracks
            .iter()
            .map(|t| MixInput {
                path: t.path.clone(),
                offset_ns: t.offset_ns,
                gain: 1.0,
                role: match (fin.settings.mix_voice_priority, t.role) {
                    (false, _) => TrackRole::Normal,
                    (true, Role::Input) => TrackRole::Voice,
                    (true, Role::Output) => TrackRole::Ducked,
                },
            })
            .collect();
        match mix_files(&inputs, &out, fin.settings.format) {
            Ok(_) => mix_ok = true,
            Err(e) => {
                let _ = std::fs::remove_file(&out);
                core.error(format!("No se pudo crear la mezcla: {e}. Se conservaron las pistas separadas."), false);
            }
        }
    }
    if mix_ok && mode == Mode::Mixed {
        for t in &fin.tracks {
            let _ = std::fs::remove_file(&t.path);
        }
    }
    let sj = SessionJson {
            id: fin.id.clone(),
            name: fin.name.clone(),
            created_at: fin.created_at.clone(),
            duration_ms: fin.duration_ms,
        };
    // Persist recovery metadata before invoking an external encoder.
    library::write_session_json(&fin.dir, &sj)?;
    if fin.settings.format == Format::Mp3 {
        for name in [library::FILE_INPUT, library::FILE_OUTPUT, library::FILE_MIX] {
            let wav = fin.dir.join(name);
            if wav.is_file() {
                match library::encode_mp3(&wav) {
                    Ok(_) => { let _ = std::fs::remove_file(&wav); }
                    Err(e) => core.error(e, false),
                }
            }
        }
    }
    let rec = library::recording_from_dir(&fin.dir).ok_or("Не удалось прочитать сохранённую запись.")?;
    if fin.tracks.len() == 1 && rec.files.len() == 1 {
        library::export_single(&fin.dir, &sj, &rec.files[0])
    } else {
        Ok(rec)
    }
}

// ───────────────────────────── hilos auxiliares ─────────────────────────────

fn events_main(core: Arc<Core>) {
    const TICK: Duration = Duration::from_millis(33);
    let mut readers = [MeterReader::new(0.9), MeterReader::new(0.9)];
    let mut last_state = Instant::now();
    let mut was_open = false;
    let mut next = Instant::now();
    while !core.shutdown.load(Acquire) {
        next += TICK;
        let now = Instant::now();
        if next > now {
            thread::sleep(next - now);
        } else {
            next = now;
        }
        let open = [core.sources[0].open.load(Acquire), core.sources[1].open.load(Acquire)];
        if open[0] || open[1] || was_open {
            let mut m = [LevelMeter::default(); 2];
            for i in 0..2 {
                if open[i] {
                    let (rms, peak) = readers[i].read(&core.sources[i].meter);
                    m[i] = LevelMeter { rms, peak, active: true };
                } else {
                    readers[i].reset();
                    core.sources[i].meter.take();
                }
            }
            // Un último cuadro en cero cuando se cierran los streams, para que la UI apague los medidores.
            core.emit(EngineEvent::Levels(Levels { input: m[0], output: m[1] }));
        }
        was_open = open[0] || open[1];
        if matches!(core.status(), Status::Recording | Status::Paused)
            && last_state.elapsed() >= Duration::from_millis(250)
        {
            last_state = Instant::now();
            core.emit_state();
        }
    }
}

fn devices_poll_main(core: Arc<Core>, ctl: Sender<Ctl>) {
    let mut waited = Duration::ZERO;
    while !core.shutdown.load(Acquire) {
        thread::sleep(Duration::from_millis(100));
        waited += Duration::from_millis(100);
        if waited < Duration::from_secs(3) {
            continue;
        }
        waited = Duration::ZERO;
        let now = devices::list_devices();
        let changed = {
            let mut last = lk(&core.last_devices);
            if *last != now {
                *last = now.clone();
                true
            } else {
                false
            }
        };
        if changed {
            core.emit(EngineEvent::DevicesChanged(now));
            let _ = ctl.send(Ctl::DevicesChanged);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::capture::new_ring;
    use crate::audio::wav::analyze;

    fn test_core() -> Arc<Core> {
        let epoch = Instant::now();
        Arc::new(Core {
            settings: Mutex::new(Settings::default()),
            persist: false,
            status: Mutex::new(StatusInner { status: Status::Recording, recording_id: None }),
            clock: Mutex::new(RunClock::new()),
            sources: [Arc::new(SourceShared::new(epoch)), Arc::new(SourceShared::new(epoch))],
            sink: Arc::new(|_| {}),
            last_devices: Mutex::new(Devices::default()),
            shutdown: AtomicBool::new(false),
        })
    }

    #[test]
    fn real_control_start_record_stop_has_only_mp3_at_every_stage() {
        use ringbuf::traits::Producer;
        struct Temp(PathBuf);
        impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); let _ = std::fs::remove_dir_all(self.0.with_extension("eco-metadata")); } }
        let root = Temp(std::env::temp_dir().join(format!("recorder-start-stop-{}-{}", std::process::id(), chrono::Local::now().timestamp_nanos_opt().unwrap())));
        std::fs::create_dir(&root.0).unwrap();
        let assert_only_mp3 = |count| {
            // read_dir includes hidden files. No filtering of directories, WAV, JSON or tmp.
            let entries: Vec<_> = std::fs::read_dir(&root.0).unwrap().map(|e|e.unwrap()).collect();
            assert_eq!(entries.len(), count);
            assert!(entries.iter().all(|e| e.file_type().unwrap().is_file() && e.path().extension().unwrap() == "mp3"));
        };
        assert_only_mp3(0);
        let core = test_core();
        core.set_status(Status::Idle, None);
        // Reproduce stale backend state: both sources + WAV, while UI selects one + MP3.
        *lk(&core.settings) = Settings { capture_input: true, capture_output: true, format: Format::Wav24,
            output_dir: root.0.to_string_lossy().into_owned(), ..Settings::default() };
        let (tx, rx) = mpsc::channel();
        let engine = Engine { core: core.clone(), ctl: tx.clone() };
        let (ptx, prx) = mpsc::channel();
        let controller = {
            let core = core.clone();
            thread::spawn(move || {
                let mut ctl = Control::new(core, tx);
                ctl.synthetic_sources = true;
                for role in Role::ALL {
                    let (prod, cons) = new_ring(48000, 1);
                    ptx.send((role, prod)).unwrap();
                    ctl.slots[role.idx()] = Some(Slot { setting_id: "default".into(), gen: 1,
                        opened: Opened { streams: vec![], sample_rate: 48000, channels: 1,
                            device_name: "Synthetic PCM, no device opened".into(), consumer: None, keepalive: false }, consumer: Some(cons) });
                }
                ctl.run(rx);
            })
        };
        let mut producers: Vec<_> = (0..2).map(|_|prx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        let mut previous_names = Vec::new();
        for (index, role) in [Role::Input, Role::Input, Role::Output].into_iter().enumerate() {
            let selected = Settings { capture_input: role == Role::Input, capture_output: role == Role::Output,
                format: Format::Mp3, output_dir: root.0.to_string_lossy().into_owned(), monitor_when_idle: true,
                ..Settings::default() };
            assert!(selected.direct_mp3());
            // This is the real Engine API -> Ctl::Start -> Control::start/start_session.
            engine.start_recording_with_settings(selected).unwrap();
            assert_eq!(engine.state().status, Status::Recording);
            assert_only_mp3(index + 1); // immediately after start, before any PCM
            let initial = std::fs::read_dir(&root.0).unwrap().map(|e|e.unwrap().path()).find(|p|!previous_names.contains(p)).unwrap();
            assert!(initial.file_name().unwrap().to_string_lossy().starts_with(if role == Role::Input {"recmic_"} else {"recsys_"}));
            let prod = &mut producers.iter_mut().find(|(r,_)|*r == role).unwrap().1;
            for _ in 0..5 {
                let pcm: Vec<f32> = (0..9600).map(|i|(i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin()*0.1).collect();
                assert_eq!(prod.push_slice(&pcm), pcm.len());
                thread::sleep(Duration::from_millis(210));
                assert_only_mp3(index + 1); // while the actual writer thread runs
            }
            assert!(std::fs::metadata(&initial).unwrap().len() > 1000);
            engine.pause_recording().unwrap();
            assert_only_mp3(index + 1);
            engine.resume_recording().unwrap();
            let saved = engine.stop_recording().unwrap(); // Ctl::Stop -> do_stop -> finalize_session
            assert_eq!(engine.state().status, Status::Idle);
            assert_eq!(saved.files.len(), 1);
            assert_eq!(PathBuf::from(&saved.files[0].path), initial);
            assert_only_mp3(index + 1);
            let decoder = PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("ffmpeg.exe");
            let mut cmd = std::process::Command::new(decoder);
            #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
            let mut child = cmd.args(["-nostdin","-v","error","-xerror","-err_detect","explode","-i"]).arg(&initial)
                .args(["-f","null","-"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
            let deadline = Instant::now()+Duration::from_secs(15);
            loop {
                if let Some(status) = child.try_wait().unwrap() { assert!(status.success()); break; }
                if Instant::now() >= deadline { let _=child.kill(); let _=child.wait(); panic!("decode timeout"); }
                thread::sleep(Duration::from_millis(20));
            }
            assert!(!previous_names.contains(&initial));
            previous_names.push(initial);
        }
        engine.shutdown();
        controller.join().unwrap();
        assert_only_mp3(3);
        assert!(root.0.exists());
    }

    #[test]
    fn single_source_mp3_writer_and_engine_finalize_use_only_reserved_file() {
        use ringbuf::traits::Producer;
        struct Temp(PathBuf);
        impl Drop for Temp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); let _ = std::fs::remove_dir_all(self.0.with_extension("eco-metadata")); } }
        let root = Temp(std::env::temp_dir().join(format!("eco6-engine-{}-{}", std::process::id(), chrono::Local::now().timestamp_nanos_opt().unwrap())));
        std::fs::create_dir(&root.0).unwrap();
        for broken in [false, true] {
            let (rec, file) = library::reserve_direct_mp3(&root.0, TrackKind::Input, 48000, 1).unwrap();
            let path = PathBuf::from(&rec.files[0].path);
            let core = test_core();
            lk(&core.settings).output_dir = root.0.to_string_lossy().into_owned();
            core.set_status(Status::Recording, Some(rec.id.clone()));
            lk(&core.clock).accumulated = Duration::from_secs(2);
            let file = if broken { drop(file); std::fs::File::open(&path).unwrap() } else { file };
            let (mut prod, cons) = new_ring(48000, 1);
            let samples: Vec<f32> = (0..4096).map(|i|(i as f32 * 0.05).sin()*0.1).collect();
            assert_eq!(prod.push_slice(&samples), samples.len());
            let track = WTrack { role: Role::Input, cons,
                writer: TrackWriter::direct_mp3(file, 48000, 1).unwrap(), sample_rate: 48000, channels: 1,
                shared: core.sources[0].clone(), offset_ns: None, failed: None };
            let (tx, _rx) = mpsc::channel();
            let engine = Engine { core: core.clone(), ctl: tx.clone() };
            assert!(engine.rename_recording(&rec.id, "blocked").is_err());
            assert!(engine.delete_recording(&rec.id).is_err());
            assert!(!engine.list_recordings().iter().any(|r|r.id == rec.id));
            let result = writer_main(core.clone(), vec![track], Arc::new(AtomicBool::new(true)), tx);
            assert_eq!(result[0].error.is_some(), broken);
            let fin = Finished { id: rec.id.clone(), name: rec.name, created_at: rec.created_at,
                dir: root.0.clone(), settings: lk(&core.settings).clone(), duration_ms: 2000,
                tracks: vec![FinTrack { role: Role::Input, path: path.clone(), offset_ns: 0 }], write_failed: broken };
            let result = finalize_session(&core, fin);
            assert_eq!(result.is_err(), broken);
            assert_eq!(core.status(), Status::Idle);
            assert!(path.exists());
            if !broken { assert_eq!(result.unwrap().files[0].path, path.to_string_lossy()); }
        }
        let files: Vec<_> = std::fs::read_dir(&root.0).unwrap().map(|e|e.unwrap().path()).collect();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|p|p.extension().unwrap() == "mp3"));
    }

    /// Un loopback que no entrega NADA (silencio del sistema sin keep-alive) debe durar lo mismo
    /// que la grabación, y la pausa no debe contar.
    #[test]
    fn silent_track_is_filled_by_clock_and_pause_is_excluded() {
        let dir = std::env::temp_dir().join("eco-engine-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("silent.wav");
        let core = test_core();
        let (_prod, cons) = new_ring(48_000, 2);
        let track = WTrack {
            role: Role::Output,
            cons,
            writer: TrackWriter::create(&path, 48_000, 2, Format::Wav24).unwrap(),
            sample_rate: 48_000,
            channels: 2,
            shared: core.sources[1].clone(),
            offset_ns: None,
            failed: None,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = mpsc::channel();
        lk(&core.clock).restart();
        let h = {
            let (c, s) = (core.clone(), stop.clone());
            thread::spawn(move || writer_main(c, vec![track], s, tx))
        };
        thread::sleep(Duration::from_millis(700));
        lk(&core.clock).pause();
        thread::sleep(Duration::from_millis(600)); // pausa: no cuenta
        lk(&core.clock).resume();
        thread::sleep(Duration::from_millis(700));
        lk(&core.clock).pause();
        stop.store(true, Release);
        let res = h.join().unwrap();
        assert!(res[0].error.is_none());
        assert_eq!(res[0].offset_ns, 0);
        let (sr, ch, dur, peak) = analyze(&path).unwrap();
        assert_eq!((sr, ch), (48_000, 2));
        assert!((dur - 1.4).abs() < 0.03, "dur {dur}");
        assert_eq!(peak, 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
