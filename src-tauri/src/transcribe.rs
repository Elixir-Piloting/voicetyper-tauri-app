use crate::config::Config;
use crate::model::WhisperHandle;

pub async fn transcribe_with_groq(audio: &[f32], config: &Config) -> Result<String, String> {
    let api_key = if config.groq_api_key.is_empty() {
        std::env::var("GROQ_API_KEY").map_err(|_| "no Groq API key".to_string())?
    } else {
        config.groq_api_key.clone()
    };

    let wav_bytes = crate::recorder::audio_to_wav(audio, config.sample_rate())
        .map_err(|e| format!("wav encode: {}", e))?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(
            reqwest::multipart::Form::new()
                .part("file", reqwest::multipart::Part::bytes(wav_bytes).file_name("audio.wav").mime_str("audio/wav").map_err(|e| e.to_string())?)
                .text("model", config.groq_stt_model.clone())
                .text("temperature", "0")
                .text("response_format", "verbose_json")
                .text("language", config.language.clone()),
        )
        .send()
        .await
        .map_err(|e| format!("groq request: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("groq STT {}: {}", status, text));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("groq parse: {}", e))?;

    let text = data
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(text)
}

pub fn transcribe_local(audio: &[f32], whisper: &WhisperHandle, lang: &str) -> Result<String, String> {
    whisper.transcribe(audio, lang)
}
