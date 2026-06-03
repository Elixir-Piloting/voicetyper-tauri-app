use tauri::{AppHandle, Emitter, Manager, State};

use crate::config::Config;
use crate::environment;
use crate::recorder::Recorder;
use crate::AppState;

fn clone_state(state: &AppState) -> AppState {
    AppState {
        config: state.config.clone(),
        recorder: state.recorder.clone(),
        is_recording: state.is_recording.clone(),
        is_processing: state.is_processing.clone(),
        whisper: state.whisper.clone(),
        hotkey_status: state.hotkey_status.clone(),
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config.lock().clone()
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    config.save();
    *state.config.lock() = config;
    Ok(())
}

#[tauri::command]
pub async fn test_groq_key(key: String) -> Result<bool, String> {
    if key.is_empty() {
        return Ok(false);
    }
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.groq.com/openai/v1/models")
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
        .map_err(|e| format!("groq test: {}", e))?;

    Ok(resp.status().is_success())
}

#[tauri::command]
pub async fn test_openrouter_key(key: String) -> Result<bool, String> {
    if key.is_empty() {
        return Ok(false);
    }
    let client = reqwest::Client::new();
    let resp = client
        .get("https://openrouter.ai/api/v1/auth/key")
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
        .map_err(|e| format!("openrouter test: {}", e))?;

    Ok(resp.status().is_success())
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut rec_lock = state.recorder.lock();
    if rec_lock.is_some() {
        return Err("already recording".to_string());
    }

    let mut recorder = Recorder::new();
    recorder.start()?;

    *state.is_recording.lock() = true;
    *rec_lock = Some(recorder);
    drop(rec_lock);

    app.emit(
        "island-state",
        serde_json::json!({"type": "state", "state": "recording"}),
    )
    .map_err(|e| format!("emit: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let (audio, config) = {
        let mut rec_lock = state.recorder.lock();
        let recorder = rec_lock.as_mut().ok_or_else(|| "not recording".to_string())?;

        *state.is_recording.lock() = false;
        *state.is_processing.lock() = true;

        app.emit(
            "island-state",
            serde_json::json!({"type": "state", "state": "processing"}),
        )
        .map_err(|e| format!("emit: {}", e))?;

        let audio = recorder.stop().ok_or_else(|| "no audio captured".to_string())?;
        let config = state.config.lock().clone();
        *rec_lock = None;
        drop(rec_lock);
        (audio, config)
    };

    if audio.len() < 1600 {
        *state.is_processing.lock() = false;
        app.emit(
            "island-state",
            serde_json::json!({"type": "state", "state": "idle"}),
        )
        .map_err(|e| format!("emit: {}", e))?;
        return Ok(String::new());
    }

    let raw = if config.use_groq {
        crate::transcribe::transcribe_with_groq(&audio, &config).await?
    } else {
        let whisper = state.whisper.lock();
        let h = whisper.as_ref().ok_or_else(|| "Whisper not loaded".to_string())?;
        let result = crate::transcribe::transcribe_local(&audio, h, &config.language);
        drop(whisper);
        result?
    };

    let raw = config.replace_text(&raw);

    if raw.is_empty() {
        *state.is_processing.lock() = false;
        app.emit(
            "island-state",
            serde_json::json!({"type": "state", "state": "idle"}),
        )
        .map_err(|e| format!("emit: {}", e))?;
        return Ok(String::new());
    }

    let env = environment::detect_environment();
    let (window_class, window_title) = environment::get_active_window(&env);

    let cleaned =
        crate::cleanup::cleanup_text(&raw, &window_class, &window_title, &config).await?;
    let cleaned = config.replace_text(&cleaned);

    crate::paste::copy_to_clipboard(&env, &cleaned)?;
    crate::paste::simulate_paste(&env, &window_class)?;

    *state.is_processing.lock() = false;

    app.emit(
        "island-state",
        serde_json::json!({"type": "state", "state": "idle"}),
    )
    .map_err(|e| format!("emit: {}", e))?;

    log::info!("✓ pasted: {:.120}", cleaned);

    emit_transcription_result(&app, &raw, &cleaned);

    Ok(cleaned)
}

#[tauri::command]
pub async fn retry_cleanup(text: String, state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config.lock().clone();
    let env = environment::detect_environment();
    let (window_class, window_title) = environment::get_active_window(&env);
    let cleaned =
        crate::cleanup::cleanup_text(&text, &window_class, &window_title, &config).await?;
    Ok(config.replace_text(&cleaned))
}

#[tauri::command]
pub async fn paste_text(text: String) -> Result<(), String> {
    let env = environment::detect_environment();
    crate::paste::copy_to_clipboard(&env, &text)?;
    crate::paste::simulate_paste(&env, "")?;
    Ok(())
}

#[tauri::command]
pub async fn download_whisper_model(app: AppHandle, model: String) -> Result<(), String> {
    log::info!("model download requested: {}", model);
    crate::model::download_model(&app, model)
}

#[tauri::command]
pub fn get_whisper_models() -> Vec<serde_json::Value> {
    crate::model::MODEL_URLS.iter().map(|(name, url, size)| {
        let path = crate::model::model_path(name);
        let downloaded = path.exists();
        serde_json::json!({
            "name": name,
            "url": url,
            "size_mb": format!("{:.0}", *size as f64 / 1_000_000.0),
            "downloaded": downloaded,
        })
    }).collect()
}

#[tauri::command]
pub fn get_hotkey_status(state: State<'_, AppState>) -> serde_json::Value {
    let status = state.hotkey_status.lock().clone();
    let cfg = state.config.lock().clone();
    serde_json::json!({
        "hotkey": cfg.hotkey,
        "status": status,
    })
}

#[tauri::command]
pub fn toggle_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let state_clone = clone_state(&state);
    if *state_clone.is_recording.lock() || *state_clone.is_processing.lock() {
        tauri::async_runtime::spawn(async move {
            let _ = stop_recording_inner(app.clone(), state_clone).await;
        });
    } else {
        tauri::async_runtime::spawn(async move {
            let app_clone = app.clone();
            let _ = start_recording_inner(app_clone, state_clone).await;
        });
    }
    Ok(())
}

/// Internal version that takes owned Arc<Mutex<>> instead of State
async fn start_recording_inner(app: AppHandle, st: AppState) -> Result<String, String> {
    let mut rec_lock = st.recorder.lock();
    if rec_lock.is_some() {
        log::warn!("start_recording: already recording");
        return Err("already recording".to_string());
    }

    let mut recorder = Recorder::new();
    recorder.start().map_err(|e| {
        log::error!("recorder.start failed: {}", e);
        e
    })?;

    *st.is_recording.lock() = true;
    *rec_lock = Some(recorder);
    drop(rec_lock);

    log::info!("recording started");

    let _ = app.emit("recording-status", serde_json::json!({"recording": true}));

    app.emit(
        "island-state",
        serde_json::json!({"type": "state", "state": "recording"}),
    )
    .map_err(|e| format!("emit: {}", e))?;

    Ok(String::new())
}

async fn stop_recording_inner(app: AppHandle, st: AppState) -> Result<String, String> {
    let (audio, config) = {
        let mut rec_lock = st.recorder.lock();
        let recorder = match rec_lock.as_mut() {
            Some(r) => r,
            None => {
                log::error!("stop_recording: not recording");
                *st.is_recording.lock() = false;
                *st.is_processing.lock() = false;
                return Err("not recording".to_string());
            }
        };

        *st.is_recording.lock() = false;
        *st.is_processing.lock() = true;

        let _ = app.emit("recording-status", serde_json::json!({"recording": false, "processing": true}));

        app.emit(
            "island-state",
            serde_json::json!({"type": "state", "state": "processing"}),
        )
        .map_err(|e| format!("emit: {}", e))?;

        let audio = recorder.stop().ok_or_else(|| {
            log::error!("recorder.stop returned None");
            "no audio captured".to_string()
        })?;
        let config = st.config.lock().clone();
        *rec_lock = None;
        drop(rec_lock);
        (audio, config)
    };

    log::info!("captured {} samples ({:.1}s)", audio.len(), audio.len() as f64 / 16000.0);

    if audio.len() < 1600 {
        log::info!("audio too short, skipping");
        *st.is_processing.lock() = false;
        let _ = app.emit("recording-status", serde_json::json!({"recording": false, "processing": false}));
        app.emit(
            "island-state",
            serde_json::json!({"type": "state", "state": "idle"}),
        )
        .map_err(|e| format!("emit: {}", e))?;
        return Ok(String::new());
    }

    let raw = if config.use_groq {
        log::info!("transcribing via Groq...");
        crate::transcribe::transcribe_with_groq(&audio, &config).await.map_err(|e| {
            log::error!("groq transcription failed: {}", e);
            e
        })?
    } else {
        log::info!("transcribing via local Whisper...");
        let whisper = st.whisper.lock();
        let h = whisper.as_ref().ok_or_else(|| {
            log::error!("Whisper not loaded");
            "Whisper not loaded".to_string()
        })?;
        let result = crate::transcribe::transcribe_local(&audio, h, &config.language);
        drop(whisper);
        result.map_err(|e| {
            log::error!("local transcription failed: {}", e);
            e
        })?
    };

    log::info!("transcribed: {:.120}", raw);
    let raw = config.replace_text(&raw);

    if raw.is_empty() {
        log::info!("transcription empty after replacements");
        *st.is_processing.lock() = false;
        let _ = app.emit("recording-status", serde_json::json!({"recording": false, "processing": false}));
        app.emit(
            "island-state",
            serde_json::json!({"type": "state", "state": "idle"}),
        )
        .map_err(|e| format!("emit: {}", e))?;
        return Ok(String::new());
    }

    let env = environment::detect_environment();
    let (window_class, window_title) = environment::get_active_window(&env);

    log::info!("cleaning up text via {}...", config.cleanup_engine);
    let cleaned = crate::cleanup::cleanup_text(&raw, &window_class, &window_title, &config).await.map_err(|e| {
        log::error!("cleanup failed: {}", e);
        e
    })?;
    let cleaned = config.replace_text(&cleaned);

    log::info!("copying to clipboard...");
    crate::paste::copy_to_clipboard(&env, &cleaned).map_err(|e| {
        log::error!("clipboard copy failed: {}", e);
        e
    })?;

    log::info!("pasting...");
    crate::paste::simulate_paste(&env, &window_class).map_err(|e| {
        log::error!("paste failed: {}", e);
        e
    })?;

    *st.is_processing.lock() = false;
    let _ = app.emit("recording-status", serde_json::json!({"recording": false, "processing": false}));

    app.emit(
        "island-state",
        serde_json::json!({"type": "state", "state": "idle"}),
    )
    .map_err(|e| format!("emit: {}", e))?;

    log::info!("✓ pasted: {:.120}", cleaned);

    emit_transcription_result(&app, &raw, &cleaned);

    Ok(cleaned)
}

fn emit_transcription_result(app: &AppHandle, raw: &str, cleaned: &str) {
    let _ = app.emit("transcription-result", serde_json::json!({
        "type": "transcription-result",
        "raw": raw,
        "cleaned": cleaned,
    }));
}

pub fn setup_hotkey(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let state: State<'_, AppState> = app.state();
    let cfg = state.config.lock().clone();
    let hotkey_str = cfg.hotkey.clone();
    drop(state);

    let (modifiers, code) = parse_hotkey(&hotkey_str)?;

    use tauri_plugin_global_shortcut::Shortcut;

    let shortcut = Shortcut::new(Some(modifiers), code);

    let status_str: &str;
    match app.global_shortcut().on_shortcut(shortcut, move |a, _shortcut, event| {
        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            let _ = a.emit("hotkey-pressed", ());
            let app2 = a.clone();
            tauri::async_runtime::spawn(async move {
                let st = clone_state(&*app2.state::<AppState>());
                if *st.is_recording.lock() || *st.is_processing.lock() {
                    if let Err(e) = stop_recording_inner(app2.clone(), st).await {
                        log::error!("hotkey stop: {}", e);
                    }
                } else {
                    if let Err(e) = start_recording_inner(app2.clone(), st).await {
                        log::error!("hotkey start: {}", e);
                    }
                }
            });
        }
    }) {
        Ok(_) => {
            log::info!("hotkey registered: {} (mods={:?}, code={:?})", hotkey_str, modifiers, code);
            status_str = "registered";
        }
        Err(e) => {
            log::error!("hotkey registration failed: {}", e);
            status_str = "failed";
        }
    }

    {
        let st: State<'_, AppState> = app.state();
        *st.hotkey_status.lock() = status_str.to_string();
    }

    let _ = app.emit("hotkey-status", serde_json::json!({
        "hotkey": hotkey_str.clone(),
        "status": status_str,
    }));

    Ok(())
}

fn parse_hotkey(s: &str) -> Result<(tauri_plugin_global_shortcut::Modifiers, tauri_plugin_global_shortcut::Code), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    let parts: Vec<&str> = s.split('+').collect();
    let mut modifiers = Modifiers::empty();
    let mut key = Code::Space;

    for part in &parts {
        let p = part.to_lowercase();
        match p.as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "super" | "win" | "cmd" | "meta" => modifiers |= Modifiers::SUPER,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            _ => {
                key = match p.as_str() {
                    "space" => Code::Space,
                    "enter" => Code::Enter,
                    "escape" => Code::Escape,
                    "tab" => Code::Tab,
                    "a" => Code::KeyA,
                    "b" => Code::KeyB,
                    "c" => Code::KeyC,
                    "d" => Code::KeyD,
                    "e" => Code::KeyE,
                    "f" => Code::KeyF,
                    "g" => Code::KeyG,
                    "h" => Code::KeyH,
                    "i" => Code::KeyI,
                    "j" => Code::KeyJ,
                    "k" => Code::KeyK,
                    "l" => Code::KeyL,
                    "m" => Code::KeyM,
                    "n" => Code::KeyN,
                    "o" => Code::KeyO,
                    "p" => Code::KeyP,
                    "q" => Code::KeyQ,
                    "r" => Code::KeyR,
                    "s" => Code::KeyS,
                    "t" => Code::KeyT,
                    "u" => Code::KeyU,
                    "v" => Code::KeyV,
                    "w" => Code::KeyW,
                    "x" => Code::KeyX,
                    "y" => Code::KeyY,
                    "z" => Code::KeyZ,
                    "f1" => Code::F1,
                    "f2" => Code::F2,
                    "f3" => Code::F3,
                    "f4" => Code::F4,
                    "f5" => Code::F5,
                    "f6" => Code::F6,
                    "f7" => Code::F7,
                    "f8" => Code::F8,
                    "f9" => Code::F9,
                    "f10" => Code::F10,
                    "f11" => Code::F11,
                    "f12" => Code::F12,
                    _ => Code::Space,
                };
            }
        }
    }

    Ok((modifiers, key))
}

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let toggle_stt =
        MenuItemBuilder::with_id("toggle-stt", "Toggle Groq / Local Whisper").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let modes_submenu = {
        let state: State<'_, AppState> = app.state();
        let cfg = state.config.lock().clone();
        let mode_names: Vec<String> = cfg.writing_modes.keys().cloned().collect();
        drop(state);

        let built_items: Vec<tauri::menu::MenuItem<tauri::Wry>> = mode_names.iter().map(|name| {
            MenuItemBuilder::with_id(format!("mode-{}", name), name.clone()).build(app)
        }).collect::<Result<Vec<_>, _>>()?;
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = built_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();

        SubmenuBuilder::new(app, "Writing Mode")
            .items(&refs)
            .build()?
    };

    let menu = MenuBuilder::new(app)
        .item(&settings_item)
        .item(&toggle_stt)
        .item(&modes_submenu)
        .separator()
        .item(&quit_item)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "settings" => {
                    if let Some(win) = app.get_webview_window("settings") {
                        let _ = win.show();
                        let _ = win.set_focus();
                        let _ = win.maximize();
                    }
                }
                "toggle-stt" => {
                    let state: State<'_, AppState> = app.state();
                    let mut cfg = state.config.lock();
                    cfg.use_groq = !cfg.use_groq;
                    cfg.save();
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {
                    if let Some(mode_name) = id.strip_prefix("mode-") {
                        let state: State<'_, AppState> = app.state();
                        let mut cfg = state.config.lock();
                        cfg.writing_mode = mode_name.to_string();
                        cfg.save();
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let cur_app = tray.app_handle().clone();
                let st: AppState = clone_state(&*cur_app.state::<AppState>());
                tauri::async_runtime::spawn(async move {
                    if *st.is_recording.lock() || *st.is_processing.lock() {
                        let _ = stop_recording_inner(cur_app, st).await;
                    } else {
                        let _ = start_recording_inner(cur_app, st).await;
                    }
                });
            }
        })
        .build(app)?;

    Ok(())
}
