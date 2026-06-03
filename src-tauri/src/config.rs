use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("voicetyper")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn models_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".local")
        .join("share")
        .join("voicetyper")
        .join("models")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Hotkey
    pub hotkey: String,
    pub push_to_talk: bool,
    pub trigger_phrase: String,

    // API keys
    pub groq_api_key: String,
    pub openrouter_key: String,

    // Transcription
    pub use_groq: bool,
    pub groq_stt_model: String,
    pub whisper_model: String,
    pub whisper_compute_type: String,
    pub language: String,
    pub sample_rate: u32,

    // Cleanup
    pub cleanup_engine: String,
    pub groq_cleanup_model: String,
    pub openrouter_model: String,
    pub ollama_url: String,
    pub ollama_model: String,

    // Replacements
    pub replacements: Vec<[String; 2]>,

    // Writing modes
    pub writing_mode_auto: bool,
    pub writing_mode: String,
    pub writing_modes: HashMap<String, WritingMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WritingMode {
    pub match_fields: MatchFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MatchFields {
    pub class: String,
    pub title: String,
}

impl Default for MatchFields {
    fn default() -> Self {
        Self {
            class: String::new(),
            title: String::new(),
        }
    }
}

impl Default for WritingMode {
    fn default() -> Self {
        Self {
            match_fields: MatchFields::default(),
        }
    }
}

pub fn default_writing_modes() -> HashMap<String, WritingMode> {
    let mut modes = HashMap::new();
    modes.insert("General".into(), WritingMode::default());
    modes.insert(
        "Email".into(),
        WritingMode {
            match_fields: MatchFields {
                class: String::new(),
                title: "gmail|mail\\.google|outlook\\.com|proton\\.me|protonmail|fastmail".into(),
            },
        },
    );
    modes.insert(
        "Code".into(),
        WritingMode {
            match_fields: MatchFields {
                class: "kitty|alacritty|foot|wezterm|gnome-terminal|ghostty|xterm|st|code-oss|cursor|opencode".into(),
                title: String::new(),
            },
        },
    );
    modes
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "ctrl+super+d".into(),
            push_to_talk: true,
            trigger_phrase: "hey voicetyper".into(),
            groq_api_key: String::new(),
            openrouter_key: String::new(),
            use_groq: true,
            groq_stt_model: "whisper-large-v3".into(),
            whisper_model: "small".into(),
            whisper_compute_type: "float32".into(),
            language: "en".into(),
            sample_rate: 16000,
            cleanup_engine: "groq".into(),
            groq_cleanup_model: "meta-llama/llama-4-scout-17b-16e-instruct".into(),
            openrouter_model: "anthropic/claude-3.5-haiku".into(),
            ollama_url: "http://localhost:11434".into(),
            ollama_model: "qwen2.5:7b-instruct".into(),
            replacements: Vec::new(),
            writing_mode_auto: true,
            writing_mode: "General".into(),
            writing_modes: default_writing_modes(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<Config>(&content) {
                        Ok(mut cfg) => {
                            cfg.migrate();
                            cfg.save();
                            return cfg;
                        }
                        Err(e) => {
                            log::warn!("config parse error: {}, using defaults", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("config read error: {}, using defaults", e);
                }
            }
        }
        let cfg = Config::default();
        cfg.save();
        cfg
    }

    pub fn sample_rate(&self) -> u32 {
        if self.sample_rate > 0 { self.sample_rate } else { 16000 }
    }

    fn migrate(&mut self) {
        // Ensure default writing modes exist and are up-to-date
        let defaults = default_writing_modes();
        for (name, mode) in defaults {
            self.writing_modes.entry(name).or_insert(mode);
        }
    }

    pub fn save(&self) {
        let dir = config_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            log::error!("failed to create config dir: {}", e);
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(config_path(), &json) {
                    log::error!("failed to write config: {}", e);
                }
            }
            Err(e) => log::error!("failed to serialize config: {}", e),
        }
    }

    pub fn models_dir() -> PathBuf {
        models_dir()
    }

    pub fn ensure_dirs() {
        if let Err(e) = fs::create_dir_all(models_dir()) {
            log::error!("failed to create models dir: {}", e);
        }
    }

    pub fn get_writing_mode(&self, window_class: &str, window_title: &str) -> (String, WritingMode) {
        if !self.writing_mode_auto {
            let name = &self.writing_mode;
            return (
                name.clone(),
                self.writing_modes.get(name).cloned().unwrap_or_default(),
            );
        }
        for (name, mode) in &self.writing_modes {
            if !mode.match_fields.class.is_empty() {
                if let Ok(re) = regex::Regex::new(&mode.match_fields.class) {
                    if re.is_match(window_class) {
                        return (name.clone(), mode.clone());
                    }
                }
            }
            if !mode.match_fields.title.is_empty() {
                if let Ok(re) = regex::Regex::new(&mode.match_fields.title) {
                    if re.is_match(window_title) {
                        return (name.clone(), mode.clone());
                    }
                }
            }
        }
        ("General".into(), self.writing_modes.get("General").cloned().unwrap_or_default())
    }

    pub fn replace_text(&self, text: &str) -> String {
        if self.replacements.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let mut result = text.to_string();
        for [from, to] in &self.replacements {
            if !from.is_empty() {
                if let Ok(re) = regex::Regex::new(&regex::escape(from)) {
                    result = re.replace_all(&result, to.as_str()).to_string();
                }
            }
        }
        result
    }
}
