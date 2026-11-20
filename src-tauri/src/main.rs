// Sin consola en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_commands;
mod commands;
mod commands_audio;
mod flyout;
mod hotkeys;
mod icons;
mod prefs;
mod tray;

use commands::AppState;
use eco_audio::types::{EngineErrorPayload, HotkeyPayload, Status};
use eco_audio::{settings, Engine, EngineEvent, EventSink};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_global_shortcut::ShortcutState;

fn emit_error(app: &AppHandle, message: String, fatal: bool) {
    let _ = app.emit("engine-error", EngineErrorPayload { message, fatal });
}

/// Ejecuta la acción del atajo y avisa a la UI (informativo).
fn run_hotkey(app: &AppHandle, action: &'static str) {
    let engine = app.state::<AppState>().engine.clone();
    let r = match (action, engine.state().status) {
        ("toggle", Status::Idle) => engine.start_recording().map(|_| ()),
        ("toggle", Status::Recording | Status::Paused) => engine.stop_recording().map(|_| ()),
        ("pause", Status::Recording) => engine.pause_recording().map(|_| ()),
        ("pause", Status::Paused) => engine.resume_recording().map(|_| ()),
        _ => Ok(()),
    };
    if let Err(e) = r {
        emit_error(app, e, false);
    }
    let _ = app.emit("hotkey", HotkeyPayload { action: action.into() });
}

fn make_sink(app: AppHandle) -> EventSink {
    Arc::new(move |ev| {
        let _ = match ev {
            EngineEvent::Levels(p) => app.emit("levels", p),
            EngineEvent::State(p) => app.emit("state", p),
            EngineEvent::RecordingSaved(p) => app.emit("recording-saved", p),
            EngineEvent::DevicesChanged(p) => app.emit("devices-changed", p),
            EngineEvent::Error(p) => app.emit("engine-error", p),
        };
    })
}

fn main() {
    // Explicit offline diagnostic entrypoint, before settings, devices, singleton,
    // autostart and hotkeys. Safe to exercise an UPX copy without touching a mic.
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "--self-test-mp3") {
        let result = (|| -> Result<(), String> {
            if args.len() != 3 { return Err("Usage: recorder_mic --self-test-mp3 <new-file.mp3>".into()); }
            let path = std::path::Path::new(&args[2]);
            if path.extension().and_then(|e|e.to_str()) != Some("mp3") { return Err("Expected a new .mp3 path".into()); }
            let file = std::fs::File::options().write(true).create_new(true).open(path).map_err(|e|e.to_string())?;
            let mut writer = eco_audio::audio::wav::TrackWriter::direct_mp3(file, 48000, 1)?;
            for _ in 0..20 {
                let pcm: Vec<f32> = (0..4800).map(|i|(i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin()*0.1).collect();
                writer.write(&pcm)?;
                writer.flush()?;
            }
            writer.finalize()?;
            println!("recorder_mic 0.1.1: synthetic MP3 PASS; no device/settings/singleton opened");
            Ok(())
        })();
        if let Err(e) = result { eprintln!("{e}"); std::process::exit(1); }
        return;
    }
    let app = tauri::Builder::default()
        // Instancia única: debe ir primero. Abrir Eco otra vez muestra la ventana existente.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|a| a == "--hidden") {
                tray::show_main(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .on_window_event(tray::on_window_event)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let action = {
                        let state = app.state::<AppState>();
                        let hk = state.hotkeys.lock().unwrap_or_else(|p| p.into_inner());
                        match hk.action_for(shortcut) {
                            Some(a) => a,
                            None => return,
                        }
                    };
                    let app = app.clone();
                    std::thread::spawn(move || run_hotkey(&app, action));
                })
                .build(),
        )
        .setup(|app| {
            let settings = hotkeys::sanitize(settings::load());
            let hk = hotkeys::Hotkeys::new(&settings);
            let engine = Engine::new(settings, make_sink(app.handle().clone()), true);
            app.manage(AppState { engine, hotkeys: std::sync::Mutex::new(hk) });
            // Si algún atajo está ocupado, la UI lo ve con `get_hotkey_status` y avisa.
            hotkeys::register_all(app.handle());
            app.manage(tray::Shell::new());
            tray::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::get_settings,
            commands::set_settings,
            commands::get_hotkey_status,
            commands::get_state,
            commands::start_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::stop_recording,
            commands::list_recordings,
            commands::rename_recording,
            commands::delete_recording,
            commands::reveal_path,
            commands::open_output_dir,
            commands::open_project,
            commands::choose_output_dir,
            commands_audio::get_input_endpoint_volume,
            commands_audio::set_input_endpoint_volume,
            app_commands::get_app_prefs,
            app_commands::set_app_prefs,
            app_commands::show_main_window,
            app_commands::hide_widget,
        ])
        .build(tauri::generate_context!())
        .expect("error al iniciar Eco");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // Si se cierra la ventana grabando, se detiene y se guarda lo grabado.
            if let Some(state) = app.try_state::<AppState>() {
                state.engine.shutdown();
            }
        }
    });
}
