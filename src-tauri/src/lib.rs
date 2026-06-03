pub mod config;
pub mod environment;
pub mod paste;
pub mod recorder;
pub mod transcribe;
pub mod cleanup;
pub mod commands;
pub mod model;
pub mod keywatcher;

use std::sync::Arc;
use parking_lot::Mutex;
use tauri::Manager;
use config::Config;
use recorder::Recorder;
use model::WhisperHandle;

pub struct AppState {
    pub config: Arc<Mutex<Config>>,
    pub recorder: Arc<Mutex<Option<Recorder>>>,
    pub is_recording: Arc<Mutex<bool>>,
    pub is_processing: Arc<Mutex<bool>>,
    pub whisper: Arc<Mutex<Option<WhisperHandle>>>,
    pub hotkey_status: Arc<Mutex<String>>,
    pub evdev_hotkey: Arc<Mutex<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let app_state = AppState {
        config: Arc::new(Mutex::new(Config::load())),
        recorder: Arc::new(Mutex::new(None)),
        is_recording: Arc::new(Mutex::new(false)),
        is_processing: Arc::new(Mutex::new(false)),
        whisper: Arc::new(Mutex::new(None)),
        hotkey_status: Arc::new(Mutex::new("detecting".to_string())),
        evdev_hotkey: Arc::new(Mutex::new("ctrl+super".to_string())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(|app| {
            let handle = app.handle();

            let st: tauri::State<'_, AppState> = app.state();
            let hotkey_str = st.config.lock().hotkey.clone();
            *st.evdev_hotkey.lock() = hotkey_str;
            drop(st);

            commands::setup_hotkey(handle)?;
            commands::setup_tray(handle)?;

            let app_clone = handle.clone();
            let st: tauri::State<'_, AppState> = app.state();
            let hk = st.evdev_hotkey.clone();
            drop(st);
            keywatcher::spawn_keywatcher(app_clone, hk);

            let st: tauri::State<'_, AppState> = app.state();
            let available = model::get_available_models();
            if let Some(model_name) = available.first() {
                let full_path = model::model_path(&model_name.replace("ggml-", "").replace(".bin", ""));
                let path = if full_path.exists() { full_path } else {
                    let dir = model::models_dir();
                    dir.join(model_name)
                };
                if path.exists() {
                    match WhisperHandle::load(&path) {
                        Ok(h) => {
                            *st.whisper.lock() = Some(h);
                            log::info!("loaded whisper model: {}", path.display());
                        }
                        Err(e) => log::warn!("failed to load {}: {}", path.display(), e),
                    }
                }
            }
            drop(st);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::test_groq_key,
            commands::test_openrouter_key,
            commands::start_recording,
            commands::stop_recording,
            commands::retry_cleanup,
            commands::paste_text,
            commands::download_whisper_model,
            commands::get_whisper_models,
            commands::toggle_recording,
            commands::get_hotkey_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running voicetyper");
}
