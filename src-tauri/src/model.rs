use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

pub const MODEL_URLS: &[(&str, &str, usize)] = &[
    ("tiny", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin", 77_700_000),
    ("tiny.en", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin", 77_700_000),
    ("base", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin", 148_000_000),
    ("base.en", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin", 148_000_000),
    ("small", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin", 488_000_000),
    ("small.en", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin", 488_000_000),
];

pub fn models_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("~/.local/share"));
    base.join("voicetyper").join("models")
}

pub fn model_path(model_name: &str) -> PathBuf {
    let fname = format!("ggml-{}.bin", model_name);
    models_dir().join(fname)
}

pub fn resolve_model_path(model_name: &str) -> Option<PathBuf> {
    let path = model_path(model_name);
    if path.exists() {
        Some(path)
    } else {
        MODEL_URLS.iter().find(|(name, _, _)| *name == model_name).and_then(|(_, url, _)| {
            let fname = url.rsplit('/').next()?;
            let path = models_dir().join(fname);
            if path.exists() { Some(path) } else { None }
        })
    }
}

pub fn get_available_models() -> Vec<String> {
    let dir = models_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".bin") {
                models.push(name);
            }
        }
    }
    models
}

fn url_for_model(model_name: &str) -> Option<&'static str> {
    MODEL_URLS.iter().find(|(name, _, _)| *name == model_name).map(|(_, url, _)| *url)
}

pub fn download_model(app: &AppHandle, model_name: String) -> Result<(), String> {
    let url = url_for_model(&model_name).ok_or_else(|| format!("unknown model: {}", model_name))?;
    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create models dir: {}", e))?;
    let fname = url.rsplit('/').next().unwrap();
    let dest = dir.join(fname);

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {}", e))?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .user_agent("voicetyper/0.1")
            .build()
            .map_err(|e| format!("client: {}", e))?;

        let resp = client.get(url)
            .send()
            .await
            .map_err(|e| format!("download request: {}", e))?;

        let total = resp.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;

        use tokio::io::AsyncWriteExt;
        let file = tokio::fs::File::create(&dest).await.map_err(|e| format!("create file: {}", e))?;
        let mut writer = tokio::io::BufWriter::new(file);
        let mut stream = resp.bytes_stream();

        use tokio_stream::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("download chunk: {}", e))?;
            writer.write_all(&chunk).await.map_err(|e| format!("write chunk: {}", e))?;
            downloaded += chunk.len() as u64;
            if total > 0 {
                let pct = (downloaded as f64 / total as f64 * 100.0) as u32;
                let _ = app.emit("download-progress", serde_json::json!({
                    "type": "download-progress",
                    "model": model_name,
                    "downloaded": downloaded,
                    "total": total,
                    "percent": pct,
                }));
            }
        }

        writer.flush().await.map_err(|e| format!("flush: {}", e))?;
        Ok(())
    })
}

pub struct WhisperHandle {
    ctx: WhisperContext,
}

unsafe impl Send for WhisperHandle {}
unsafe impl Sync for WhisperHandle {}

impl WhisperHandle {
    pub fn load(model_path: &std::path::Path) -> Result<Self, String> {
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(model_path, params)
            .map_err(|e| format!("whisper init: {:?}", e))?;
        Ok(WhisperHandle { ctx })
    }

    pub fn transcribe(&self, audio: &[f32], lang: &str) -> Result<String, String> {
        let mut state = self.ctx.create_state()
            .map_err(|e| format!("whisper state: {:?}", e))?;

        let lang = if lang.is_empty() || lang == "auto" { None } else { Some(lang) };

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
        params.set_language(lang);
        params.set_n_threads(4);
        params.set_no_timestamps(true);

        state.full(params, audio)
            .map_err(|e| format!("whisper full: {:?}", e))?;

        let n_segments = state.full_n_segments();
        let mut text = String::new();
        for i in 0..n_segments {
            if let Some(seg) = state.get_segment(i) {
                text.push_str(seg.to_str().unwrap_or(""));
            }
        }

        Ok(text.trim().to_string())
    }
}


