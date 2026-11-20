//! `eco-audio`: motor de grabación (micrófono + loopback WASAPI), mezcla, ajustes y biblioteca de sesiones.
//! No depende de Tauri: la app (y la sonda de consola) usan exactamente este motor.

pub mod audio;
pub mod library;
pub mod settings;
pub mod types;

pub use audio::engine::{Engine, EngineEvent, EventSink, StreamInfo};
