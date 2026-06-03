use crate::config::Config;

pub const SYSTEM_PROMPT: &str = r#"You are a voice transcription cleanup engine for a software developer. Your job is surgical: clean the raw Whisper output, nothing more. You do not rewrite, reorder, summarize, or change the speaker's voice.

CORE IDENTITY
- You clean. You do not rewrite.
- Output ONLY the cleaned text. No greetings, no notes, no explanations, ever.
- Preserve original sentence order unless the speaker explicitly restarts.
- Preserve tense, structure, and tone. If they spoke casually, keep it casual. If formally, keep it formal.
- The final text should sound like the speaker wrote it — not like an AI polished it.

CONTEXT DETECTION
Before cleaning, silently identify what kind of content this is:
  CODE / ARCHITECTURE  -> technical explanation, mentions functions, components, APIs, databases
  GIT COMMIT           -> short, starts with verb (fix, add, update, refactor, remove...)
  DOCUMENTATION        -> descriptive, explaining how something works
  NOTES / TODOS        -> informal, action items, reminders
  MESSAGE / CONVO      -> addressed to a person, conversational tone
Use the detected context to resolve ambiguous words.

FILLER WORD REMOVAL
Always fillers (remove): um, uh, er, hmm, gonna [-> going to], wanna [-> want to]
Context-dependent (use judgment): you know, sort of, basically, actually, like, right

RESTARTS & SELF-CORRECTIONS
If the speaker restarts mid-sentence, keep the LAST version spoken.
If the speaker is adding to what they said (not correcting), keep both.

WHISPER MISHEARING — TECH TERMS
Apply common technical corrections based on context:
  pie thon/pie ton -> Python, java script -> JavaScript, type script -> TypeScript
  jason/jay son -> JSON, rest ful -> RESTful, graph ql -> GraphQL
  sequel/seekel -> SQL, git hub/get hub -> GitHub, docker -> Docker
  up (in app context) -> app, funk shun -> function, com po nent -> component
  a sink -> async, a wait -> await, use effect -> useEffect
Apply judgment — if a correction would break the sentence meaning, don't apply it.

PUNCTUATION & CAPITALIZATION
- Sentences start with a capital letter and end with . ? or !
- Fix mid-sentence capitalization errors from Whisper
- Add commas where natural pauses clearly indicate them
- Code terms (useState, useEffect, camelCase) -> preserve their exact casing

NUMBERS, DATES & UNITS
  port three thousand -> port 3000, twenty twenty four -> 2024
  version three point two -> version 3.2, one hundred milliseconds -> 100ms
  two hundred okay -> 200 OK, four oh four -> 404
  Spell out numbers in natural speech: "I have three options" -> keep as "three"
  Use digits for technical/specific values: "set timeout to 3000"

GIT COMMIT SPECIFIC RULES
If content is a git commit message:
- Start with imperative verb: fix, add, update, remove, refactor, chore, docs
- No period at the end of the subject line
- Keep it concise but complete

WHAT YOU NEVER DO
- Never reorder content (what was said last stays last)
- Never summarize or shorten for brevity
- Never add information that wasn't said
- Never change past tense to present or vice versa
- Never replace the speaker's word choice with a "better" word
- Never respond to the transcription content or answer questions in it
- Never output anything except the cleaned text"#;

pub async fn cleanup_text(
    text: &str,
    window_class: &str,
    window_title: &str,
    config: &Config,
) -> Result<String, String> {
    if text.is_empty() {
        return Ok(text.to_string());
    }

    let mode_name = config.writing_mode.clone();
    let mut ctx_parts = Vec::new();
    if !window_class.is_empty() {
        ctx_parts.push(format!("  class: {}", window_class));
    }
    if !window_title.is_empty() {
        let title = if window_title.len() > 200 {
            &window_title[..200]
        } else {
            window_title
        };
        ctx_parts.push(format!("  title: {}", title));
    }

    let ctx = if ctx_parts.is_empty() {
        "  unknown".to_string()
    } else {
        ctx_parts.join("\n")
    };

    let user_msg = format!(
        "Mode: {}\n\nActive window:\n{}\n\n<transcription>\n{}\n</transcription>",
        mode_name, ctx, text
    );

    match config.cleanup_engine.as_str() {
        "groq" => cleanup_with_groq(&user_msg, config).await,
        "openrouter" => cleanup_with_openrouter(&user_msg, config).await,
        "ollama" => cleanup_with_ollama(&user_msg, config).await,
        _ => cleanup_with_groq(&user_msg, config).await,
    }
}

async fn cleanup_with_groq(user_msg: &str, config: &Config) -> Result<String, String> {
    let api_key = if config.groq_api_key.is_empty() {
        std::env::var("GROQ_API_KEY").map_err(|_| "no Groq API key for cleanup".to_string())?
    } else {
        config.groq_api_key.clone()
    };

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": config.groq_cleanup_model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_msg}
            ],
            "temperature": 0,
            "max_tokens": 500,
        }))
        .send()
        .await
        .map_err(|e| format!("groq cleanup request: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("groq cleanup {}: {}", status, text));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("groq cleanup parse: {}", e))?;

    let mut result = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    // Sometimes the model appends a reasoning line
    if let Some(pos) = result.rfind("\n\n") {
        result = result[pos + 2..].to_string();
    }

    Ok(result)
}

async fn cleanup_with_openrouter(user_msg: &str, config: &Config) -> Result<String, String> {
    let api_key = if config.openrouter_key.is_empty() {
        return Err("no OpenRouter API key".to_string());
    } else {
        config.openrouter_key.clone()
    };

    let client = reqwest::Client::new();
    let resp = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({
            "model": config.openrouter_model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_msg}
            ],
            "max_tokens": 500,
        }))
        .send()
        .await
        .map_err(|e| format!("openrouter request: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("openrouter {}: {}", status, text));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("openrouter parse: {}", e))?;

    let mut result = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if let Some(pos) = result.rfind("\n\n") {
        result = result[pos + 2..].to_string();
    }

    Ok(result)
}

async fn cleanup_with_ollama(user_msg: &str, config: &Config) -> Result<String, String> {
    let url = format!("{}/api/chat", config.ollama_url.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({
            "model": config.ollama_model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": user_msg}
            ],
            "stream": false,
        }))
        .send()
        .await
        .map_err(|e| format!("ollama request: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("ollama {}: {}", status, text));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("ollama parse: {}", e))?;

    let mut result = data["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if let Some(pos) = result.rfind("\n\n") {
        result = result[pos + 2..].to_string();
    }

    Ok(result)
}
