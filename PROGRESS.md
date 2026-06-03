# VoiceTyper Tauri Port — Progress

## Session 1 (2026-06-03)

### ✅ Completed
- [x] Project scaffold (Cargo.toml, tauri.conf.json, package.json, vite.config.js, build.rs)
- [x] config.rs — Config struct, serde load/save at `~/.config/voicetyper/config.json`
- [x] environment.rs — DE detection (Hyprland/Sway/KDE/GNOME/GenericWayland/X11), window detection, TERMINAL_CLASSES
- [x] paste.rs — Clipboard (wl-copy/xclip) + paste (wtype/xdotool) per DE, terminal-aware Ctrl+Shift+V
- [x] recorder.rs — cpal audio capture with real-time amplitude extraction for waveform
- [x] transcribe.rs — Groq API (multipart) + whisper-rs placeholder for local STT
- [x] cleanup.rs — SYSTEM_PROMPT + Groq/OpenRouter/Ollama HTTP cleanup, writing mode context injection
- [x] commands.rs — All Tauri IPC commands (get_config, save_config, test keys, start/stop recording, etc.)
- [x] main.rs/lib.rs — App entry, hotkey setup, tray setup
- [x] Frontend: index.html (settings with 6 tabs), dynamic-island.html (overlay with waveform), styles.css, app.js
- [x] GitHub Actions workflow — tauri-action for AppImage + deb + rpm
- [x] Project compiles clean (cargo check passes)

### 🔲 Remaining
- [ ] Test full build (`cargo build`)
- [ ] Add whisper-rs dependency for local STT (GGUF model auto-download)
- [ ] Implement model download with progress (%)
- [ ] Test on actual Hyprland/Sway setup
- [ ] Verify Dynamic Island position (bottom-center)
- [ ] Verify tray icon works (left-click toggle, right-click menu)
- [ ] Verify hotkey (Ctrl+Super) starts/stops recording
- [ ] Verify Groq API transcription + cleanup pipeline end-to-end
- [ ] Verify clipboard + paste in various environments
- [ ] Add install script / desktop files
- [ ] Final clean: remove dead code warnings, verify all config fields persist correctly

### Architecture
```
voicetyper-tauri-app/
├── Cargo.toml                     # Rust deps
├── tauri.conf.json                # Tauri config (2 windows, tray)
├── package.json                   # Vite + Tauri CLI
├── vite.config.js
├── src/
│   ├── index.html                 # Settings window (6 tabs)
│   ├── dynamic-island.html        # Bottom-center overlay with waveform
│   ├── styles.css                 # Dark theme
│   └── app.js                     # Settings logic
├── src-tauri/
│   ├── build.rs
│   ├── icons/                     # RGBA PNG icons
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs                # Entry point
│       ├── lib.rs                 # Module decls, AppState
│       ├── config.rs              # Config ~/.config/voicetyper/config.json
│       ├── environment.rs         # DE detection
│       ├── paste.rs               # Clipboard + paste
│       ├── recorder.rs            # cpal audio capture
│       ├── transcribe.rs          # Groq STT + whisper-rs placeholder
│       ├── cleanup.rs             # LLM cleanup (SYSTEM_PROMPT)
│       └── commands.rs            # Tauri IPC commands
└── .github/workflows/build.yml    # AppImage + deb + rpm
```
