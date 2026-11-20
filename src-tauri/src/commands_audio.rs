//! Comandos de audio de voz: volumen del micrófono en Windows (IAudioEndpointVolume).
//! Solo lee/cambia el nivel del dispositivo de captura elegido en Ajustes; no toca el volumen de reproducción.

use crate::commands::AppState;
use eco_audio::audio::endpoint::{self, EndpointVolume};
use tauri::State;

async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("Error interno: {e}"))?
}

/// Volumen y silencio del micrófono seleccionado en Windows.
#[tauri::command]
pub async fn get_input_endpoint_volume(state: State<'_, AppState>) -> Result<EndpointVolume, String> {
    let id = state.engine.settings().input_id;
    blocking(move || endpoint::get_input_endpoint_volume(&id)).await
}

/// Fija el volumen (0..1) del micrófono seleccionado (y le quita el silencio si `level` > 0).
#[tauri::command]
pub async fn set_input_endpoint_volume(state: State<'_, AppState>, level: f32) -> Result<EndpointVolume, String> {
    let id = state.engine.settings().input_id;
    blocking(move || endpoint::set_input_endpoint_volume(&id, level)).await
}
