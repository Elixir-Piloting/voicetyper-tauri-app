use std::collections::HashSet;
use std::sync::Arc;
use evdev::KeyCode;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{clone_state, start_recording_inner, stop_recording_inner};
use crate::AppState;

const KEY_LEFTCTRL: u16 = 29;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTMETA: u16 = 126;
const KEY_LEFTALT: u16 = 56;
const KEY_RIGHTALT: u16 = 100;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;

fn is_modifier(code: u16) -> bool {
    matches!(code, KEY_LEFTCTRL | KEY_RIGHTCTRL | KEY_LEFTMETA | KEY_RIGHTMETA | KEY_LEFTALT | KEY_RIGHTALT | KEY_LEFTSHIFT | KEY_RIGHTSHIFT)
}

fn code_to_mod_name(code: &u16) -> Option<&'static str> {
    match code {
        &KEY_LEFTCTRL | &KEY_RIGHTCTRL => Some("ctrl"),
        &KEY_LEFTMETA | &KEY_RIGHTMETA => Some("super"),
        &KEY_LEFTALT | &KEY_RIGHTALT => Some("alt"),
        &KEY_LEFTSHIFT | &KEY_RIGHTSHIFT => Some("shift"),
        _ => None,
    }
}

pub fn spawn_keywatcher(app: AppHandle, hotkey: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let mut handles: Vec<evdev::Device> = Vec::new();
        for (path, dev) in evdev::enumerate() {
            let name = dev.name().unwrap_or("").to_lowercase();
            if !name.contains("keyboard") && !name.contains("kbd") {
                continue;
            }
            match evdev::Device::open(&path) {
                Ok(dev) => {
                    if dev.supported_keys().map_or(false, |keys| keys.contains(KeyCode::KEY_LEFTCTRL)) {
                        log::info!("keywatcher: opened {}", path.display());
                        handles.push(dev);
                    }
                }
                Err(e) => log::warn!("keywatcher: can't open {}: {}", path.display(), e),
            }
        }

        if handles.is_empty() {
            log::error!("keywatcher: no keyboard devices found (try: sudo usermod -aG input $USER)");
            return;
        }

        let mut pressed: HashSet<u16> = HashSet::new();

        loop {
            let mut events = Vec::new();
            for dev in &mut handles {
                if let Ok(evts) = dev.fetch_events() {
                    events.extend(evts);
                }
            }

            for ev in &events {
                if ev.event_type() != evdev::EventType::KEY {
                    continue;
                }
                let code = ev.code();
                let val = ev.value();

                if is_modifier(code) {
                    if val == 1 {
                        pressed.insert(code);
                    } else if val == 0 {
                        pressed.remove(&code);
                    }
                    continue;
                }

                if val != 1 {
                    continue;
                }

                let hk = hotkey.lock().clone();
                let parts: Vec<&str> = hk.split('+').collect();

                let held: HashSet<&str> = pressed.iter().filter_map(code_to_mod_name).collect();
                let want: HashSet<&str> = parts.iter()
                    .map(|p| -> &str {
                        match *p {
                            "control" => "ctrl",
                            "win" | "cmd" | "meta" => "super",
                            other => other,
                        }
                    })
                    .filter(|p| matches!(*p, "ctrl" | "super" | "alt" | "shift"))
                    .collect();

                if held != want {
                    continue;
                }

                let key_part = parts.iter().find(|p| {
                    !matches!(**p, "ctrl" | "super" | "alt" | "shift" | "control" | "win" | "cmd" | "meta")
                });

                let match_key = match key_part {
                    Some(&"space") => code == 57,
                    Some(&"enter") => code == 28,
                    Some(k) if k.len() == 1 => {
                        let letter = k.to_ascii_lowercase().chars().next().unwrap();
                        let expected = 30 + (letter as u8 - b'a') as u16;
                        code == expected
                    }
                    Some(_) => false,
                    None => true,
                };

                if !match_key {
                    continue;
                }

                log::info!("keywatcher: hotkey match, toggling recording");
                let _ = app.emit("hotkey-pressed", ());

                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    let st = clone_state(&*app2.state::<AppState>());
                    if *st.is_recording.lock() || *st.is_processing.lock() {
                        if let Err(e) = stop_recording_inner(app2.clone(), st).await {
                            log::error!("keywatcher stop: {}", e);
                        }
                    } else {
                        if let Err(e) = start_recording_inner(app2.clone(), st).await {
                            log::error!("keywatcher start: {}", e);
                        }
                    }
                });
            }

            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
}
