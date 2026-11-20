//! Comandos de la bandeja y las preferencias de la app.

use crate::prefs::AppPrefs;
use crate::tray;
use tauri::AppHandle;

async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("Error interno: {e}"))?
}

#[tauri::command]
pub async fn get_app_prefs(app: AppHandle) -> Result<AppPrefs, String> {
    blocking(move || Ok(tray::current_prefs(&app))).await
}

#[tauri::command]
pub async fn set_app_prefs(app: AppHandle, prefs: AppPrefs) -> Result<AppPrefs, String> {
    blocking(move || tray::apply_prefs(&app, &prefs)).await
}

/// «Abrir Eco» desde el flyout: muestra la ventana principal y cierra el flyout.
#[tauri::command]
pub async fn show_main_window(app: AppHandle) -> Result<(), String> {
    tray::show_main(&app);
    Ok(())
}

/// Esc en el flyout.
#[tauri::command]
pub async fn hide_widget(app: AppHandle) -> Result<(), String> {
    tray::hide_widget(&app, false);
    Ok(())
}
