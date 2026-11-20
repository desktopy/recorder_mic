//! Comandos Tauri (`invoke`). Todos son `async` y delegan en el motor desde hilos de trabajo:
//! los comandos síncronos de Tauri correrían en el hilo principal y congelarían la ventana.

use crate::hotkeys;
use eco_audio::types::*;
use eco_audio::Engine;
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub struct AppState {
    pub engine: Engine,
    pub hotkeys: std::sync::Mutex<crate::hotkeys::Hotkeys>,
}

async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("Error interno: {e}"))?
}

#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Devices, String> {
    let e = state.engine.clone();
    blocking(move || Ok(e.list_devices())).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.engine.settings())
}

#[tauri::command]
pub async fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let e = state.engine.clone();
    blocking(move || {
        let new = settings.validated();
        let prev = e.settings();
        // Primero los atajos: si el nuevo no se puede registrar, no se guarda nada y se conserva el anterior.
        hotkeys::apply(&app, &new.hotkey_toggle, &new.hotkey_pause)?;
        e.set_settings(new).inspect_err(|_| {
            let _ = hotkeys::apply(&app, &prev.hotkey_toggle, &prev.hotkey_pause);
        })
    })
    .await
}

#[tauri::command]
pub async fn get_hotkey_status(app: AppHandle) -> Result<hotkeys::HotkeyStatus, String> {
    Ok(hotkeys::status(&app))
}

#[tauri::command]
pub async fn get_state(state: State<'_, AppState>) -> Result<EngineState, String> {
    Ok(state.engine.state())
}

#[tauri::command]
pub async fn start_recording(state: State<'_, AppState>, settings: Option<Settings>) -> Result<EngineState, String> {
    let e = state.engine.clone();
    blocking(move || match settings {
        Some(settings) => e.start_recording_with_settings(settings),
        None => e.start_recording(),
    }).await
}

#[tauri::command]
pub async fn pause_recording(state: State<'_, AppState>) -> Result<EngineState, String> {
    let e = state.engine.clone();
    blocking(move || e.pause_recording()).await
}

#[tauri::command]
pub async fn resume_recording(state: State<'_, AppState>) -> Result<EngineState, String> {
    let e = state.engine.clone();
    blocking(move || e.resume_recording()).await
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<Recording, String> {
    let e = state.engine.clone();
    blocking(move || e.stop_recording()).await
}

#[tauri::command]
pub async fn list_recordings(state: State<'_, AppState>) -> Result<Vec<Recording>, String> {
    let e = state.engine.clone();
    blocking(move || Ok(e.list_recordings())).await
}

#[tauri::command]
pub async fn rename_recording(state: State<'_, AppState>, id: String, name: String) -> Result<Recording, String> {
    let e = state.engine.clone();
    blocking(move || e.rename_recording(&id, &name)).await
}

#[tauri::command]
pub async fn delete_recording(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let e = state.engine.clone();
    blocking(move || e.delete_recording(&id)).await
}

#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), String> {
    blocking(move || {
        let p = PathBuf::from(&path);
        if !p.exists() {
            return Err("El archivo ya no existe.".into());
        }
        tauri_plugin_opener::reveal_item_in_dir(&p).map_err(|e| format!("No se pudo abrir el Explorador: {e}"))
    })
    .await
}

#[tauri::command]
pub async fn open_output_dir(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.engine.settings().output_dir;
    blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear la carpeta «{dir}»: {e}"))?;
        app.opener()
            .open_path(dir, None::<&str>)
            .map_err(|e| format!("No se pudo abrir la carpeta: {e}"))
    })
    .await
}

#[tauri::command]
pub fn open_project(app: AppHandle) -> Result<(), String> {
    app.opener().open_url("https://github.com/EndingVE/Eco-Windows-Recorder", None::<&str>)
        .map_err(|e| format!("Не удалось открыть сайт проекта: {e}"))
}

#[tauri::command]
pub async fn choose_output_dir(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let engine = state.engine.clone();
    blocking(move || {
        let mut s = engine.settings();
        let mut dlg = app.dialog().file().set_title("Elegir carpeta de grabaciones");
        if std::path::Path::new(&s.output_dir).is_dir() {
            dlg = dlg.set_directory(&s.output_dir);
        }
        let Some(picked) = dlg.blocking_pick_folder() else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|e| format!("Carpeta no válida: {e}"))?
            .to_string_lossy()
            .into_owned();
        s.output_dir = path.clone();
        engine.set_settings(s)?;
        Ok(Some(path))
    })
    .await
}
