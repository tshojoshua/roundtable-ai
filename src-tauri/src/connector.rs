//! connector.rs — Direct in-app AI provider connector
//!
//! Routes each model to its provider and calls the API directly.
//! Handles: Anthropic, xAI/Grok, Google Gemini, OpenAI, GitHub Copilot,
//!          Mistral, ERIN (custom Transformers API), Ollama (configurable)

use crate::auth::AuthState;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// ─────────────────────────────────────────
// PROVIDER ROUTING
// ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Provider {
    Anthropic,
    Gemini,
    OpenAI,
    GitHubCopilot,
    XAI,
    Mistral,
    Erin,
    Ollama,
    AnthropicWeb,   // claude.ai session
    XAIWeb,         // grok.com session
}

pub fn route_model(model_id: &str) -> (Provider, &'static str) {
    match model_id {
        m if m.starts_with("claude") =>
            (Provider::Anthropic, "https://api.anthropic.com/v1"),
        m if m.starts_with("gemini") =>
            (Provider::Gemini, "https://generativelanguage.googleapis.com/v1beta/openai"),
        m if m.starts_with("grok") =>
            (Provider::XAI, "https://api.x.ai/v1"),
        m if m.starts_with("gpt") || m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") =>
            (Provider::OpenAI, "https://api.openai.com/v1"),
        m if m.contains("copilot") || m.starts_with("github/") =>
            (Provider::GitHubCopilot, "https://api.githubcopilot.com"),
        m if m.starts_with("mistral") || m.starts_with("mixtral") =>
            (Provider::Mistral, "https://api.mistral.ai/v1"),
        m if m == "erin" || m.starts_with("erin-") || m.starts_with("qwen") =>
            (Provider::Erin, "http://10.1.1.19:5000"),
        _ => (Provider::Ollama, "http://localhost:11434/v1"),
    }
}

fn provider_key_id(provider: &Provider) -> &'static str {
    match provider {
        Provider::Anthropic    => "anthropic",
        Provider::Gemini       => "google",
        Provider::OpenAI       => "openai",
        Provider::GitHubCopilot => "github-copilot",
        Provider::XAI          => "xai",
        Provider::Mistral      => "mistral",
        Provider::Erin         => "erin",
        Provider::Ollama       => "ollama",
        Provider::AnthropicWeb => "anthropic-web",
        Provider::XAIWeb       => "xai-web",
    }
}

async fn build_auth_header(
    provider: &Provider,
    auth: &AuthState,
) -> Result<Option<(&'static str, String)>, String> {
    match provider {
        Provider::Erin | Provider::Ollama => Ok(None),
        Provider::Anthropic => {
            let key = auth.get_key("anthropic").await
                .ok_or("No Claude API key. Go to ⚙️ Providers → Claude (API Key) → Save.")?;
            Ok(Some(("x-api-key", key)))
        }
        Provider::Gemini => {
            let key = auth.get_key("google").await
                .ok_or("No Gemini API key. Go to ⚙️ Providers → Gemini → Get API Key.")?;
            Ok(Some(("Authorization", format!("Bearer {}", key))))
        }
        Provider::GitHubCopilot => {
            let key = auth.get_key("github-copilot").await
                .ok_or("No GitHub Copilot PAT. Go to ⚙️ Providers → GitHub Copilot → Get API Key.")?;
            Ok(Some(("Authorization", format!("Bearer {}", key))))
        }
        p => {
            let key_id = provider_key_id(p);
            let key = auth.get_key(key_id).await
                .ok_or_else(|| format!("No API key for {}. Go to ⚙️ Providers.", key_id))?;
            Ok(Some(("Authorization", format!("Bearer {}", key))))
        }
    }
}

fn extra_headers(provider: &Provider) -> Vec<(&'static str, &'static str)> {
    match provider {
        Provider::Anthropic => vec![("anthropic-version", "2023-06-01")],
        Provider::GitHubCopilot => vec![
            ("Editor-Version", "roundtable-ai/1.0"),
            ("Editor-Plugin-Version", "roundtable-ai/1.0"),
            ("Copilot-Integration-Id", "vscode-chat"),
        ],
        _ => vec![],
    }
}

// ─────────────────────────────────────────
// ERIN CUSTOM API
// ─────────────────────────────────────────

async fn call_erin(
    client: &Client,
    auth: &AuthState,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let base = auth.get_key("erin-endpoint").await
        .unwrap_or_else(|| "http://10.1.1.19:5000".into());

    // Send only the last user message to ERIN — she has her own identity
    // and system prompt already. Injecting rules causes her to echo them back.
    // Build context from prior messages, send latest as the message.
    let last_user = messages.iter().rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    // Build conversation context from non-user messages (AI responses so far)
    let context: Vec<String> = messages.iter()
        .filter(|m| m.role != "user" && m.role != "system" && !m.content.is_empty())
        .map(|m| format!("[{}]: {}", m.role, m.content))
        .collect();
    let context_str = if context.is_empty() {
        String::new()
    } else {
        context.join("\n\n")
    };

    let mut body = json!({
        "message": last_user,
        "conversation_id": uuid_v4()
    });
    if !context_str.is_empty() {
        body["context"] = serde_json::Value::String(context_str);
    }

    let resp: serde_json::Value = client
        .post(format!("{}/api/chat", base))
        .header("Content-Type", "application/json")
        .json(&body)
        .send().await
        .map_err(|e| format!("ERIN unreachable at {}: {}", base, e))?
        .json().await
        .map_err(|e| format!("ERIN response parse error: {}", e))?;

    Ok(resp["response"].as_str()
        .unwrap_or("(ERIN did not respond)")
        .to_string())
}

// ─────────────────────────────────────────
// OLLAMA (configurable)
// ─────────────────────────────────────────

async fn call_ollama(
    client: &Client,
    auth: &AuthState,
    model_id: &str,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let base = auth.get_key("ollama-endpoint").await
        .unwrap_or_else(|| "http://localhost:11434".into());
    let model = auth.get_key("ollama-model").await
        .unwrap_or_else(|| model_id.to_string());

    let api_msgs: Vec<serde_json::Value> = messages.iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();

    let resp: serde_json::Value = client
        .post(format!("{}/v1/chat/completions", base))
        .json(&json!({
            "model": model,
            "messages": api_msgs,
            "stream": false
        }))
        .send().await
        .map_err(|e| format!("Ollama unreachable at {}: {}", base, e))?
        .json().await
        .map_err(|e| format!("Ollama parse error: {}", e))?;

    Ok(resp["choices"][0]["message"]["content"]
        .as_str().unwrap_or("(no response)").to_string())
}

// ─────────────────────────────────────────
// CLAUDE WEB SESSION (claude.ai)
// ─────────────────────────────────────────

pub async fn call_claude_web(
    client: &Client,
    auth: &AuthState,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let session_key = auth.get_key("anthropic-web").await
        .ok_or("No Claude session. Go to ⚙️ Providers → Re-import Claude Session.")?;

    // Step 1: Create a new conversation
    let org_resp: serde_json::Value = client
        .get("https://claude.ai/api/organizations")
        .header("Cookie", format!("sessionKey={}", session_key))
        .header("User-Agent", "Mozilla/5.0")
        .send().await.map_err(|e| format!("claude.ai org fetch: {}", e))?
        .json().await.map_err(|e| format!("claude.ai org parse: {}", e))?;

    let org_id = org_resp[0]["uuid"].as_str()
        .ok_or("claude.ai: could not get org ID — session may be expired, re-import it")?
        .to_string();

    let conv_id = uuid_v4();
    client.post(format!("https://claude.ai/api/organizations/{}/chat_conversations", org_id))
        .header("Cookie", format!("sessionKey={}", session_key))
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0")
        .json(&json!({ "uuid": conv_id, "name": "" }))
        .send().await.map_err(|e| format!("claude.ai conv create: {}", e))?;

    // Step 2: Send message with SSE, collect completion
    let prompt = messages.iter()
        .filter(|m| m.role == "user")
        .last()
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let resp = client
        .post(format!("https://claude.ai/api/organizations/{}/chat_conversations/{}/completion", org_id, conv_id))
        .header("Cookie", format!("sessionKey={}", session_key))
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0")
        .json(&json!({
            "prompt": prompt,
            "model": "claude-opus-4-5",
            "attachments": [],
            "files": []
        }))
        .send().await.map_err(|e| format!("claude.ai completion: {}", e))?;

    let body = resp.text().await.map_err(|e| format!("claude.ai read body: {}", e))?;

    // Parse SSE stream — find completion_stop event
    let mut result = String::new();
    for line in body.lines() {
        if line.starts_with("data: ") {
            let data = &line[6..];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                if v["type"] == "content_block_delta" {
                    if let Some(t) = v["delta"]["text"].as_str() {
                        result.push_str(t);
                    }
                }
                if v["type"] == "completion" {
                    if let Some(t) = v["completion"].as_str() {
                        result = t.to_string();
                    }
                }
            }
        }
    }

    if result.is_empty() {
        return Err("claude.ai returned empty response — session may be expired, try re-importing".into());
    }
    Ok(result)
}

// ─────────────────────────────────────────
// GROK WEB SESSION (grok.com)
// ─────────────────────────────────────────

pub async fn call_grok_web(
    client: &Client,
    auth: &AuthState,
    messages: &[ChatMessage],
) -> Result<String, String> {
    let token_data = auth.get_key("xai-web").await
        .ok_or("No Grok session. Go to ⚙️ Providers → Import from Grok Desktop.")?;
    let (auth_token, ct0) = if token_data.contains('|') {
        let parts: Vec<&str> = token_data.splitn(2, '|').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        (token_data.clone(), String::new())
    };
    let prompt = messages.iter()
        .map(|m| format!("[{}]: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let cookie = if ct0.is_empty() {
        format!("sso={}", auth_token)
    } else {
        format!("auth_token={}; ct0={}", auth_token, ct0)
    };
    let resp: serde_json::Value = client
        .post("https://grok.com/api/rpc/message/send")
        .header("Cookie", cookie)
        .header("Content-Type", "application/json")
        .header("Referer", "https://grok.com/")
        .json(&json!({
            "message": prompt, "modelName": "grok-3",
            "conversationId": null, "returnSearchResults": false
        }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    Ok(resp["result"]["content"].as_str()
        .or_else(|| resp["message"].as_str())
        .unwrap_or("(no response from Grok web)").to_string())
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (t & 0xffffffff) as u32, ((t >> 32) & 0xffff) as u16,
        ((t >> 48) & 0x0fff) as u16, (0x8000 | ((t >> 60) & 0x3fff)) as u16,
        t as u64 & 0xffffffffffff)
}

// ─────────────────────────────────────────
// MAIN CALL (non-streaming)
// ─────────────────────────────────────────

pub async fn call_model(
    client: &Client,
    auth: &AuthState,
    model_id: &str,
    messages: &[ChatMessage],
    max_tokens: u32,
) -> Result<String, String> {
    // Special cases with custom APIs
    if model_id == "erin" || model_id.starts_with("erin-") { return call_erin(client, auth, messages).await; }
    if model_id.starts_with("ollama/") || model_id == "ollama" { return call_ollama(client, auth, model_id, messages).await; }
    if model_id == "claude-web" { return call_claude_web(client, auth, messages).await; }
    if model_id == "grok-web" { return call_grok_web(client, auth, messages).await; }

    let (provider, api_base) = route_model(model_id);
    let auth_hdr = build_auth_header(&provider, auth).await?;

    let api_msgs: Vec<serde_json::Value> = messages.iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();

    let mut req = client
        .post(format!("{}/chat/completions", api_base))
        .json(&json!({
            "model": model_id,
            "messages": api_msgs,
            "max_tokens": max_tokens,
            "stream": false
        }));

    if let Some((hdr, val)) = auth_hdr { req = req.header(hdr, val); }
    for (k, v) in extra_headers(&provider) { req = req.header(k, v); }

    let resp: serde_json::Value = req.send().await
        .map_err(|e| format!("Request failed ({}): {}", model_id, e))?
        .json().await
        .map_err(|e| format!("JSON parse failed ({}): {}", model_id, e))?;

    if let Some(err) = resp.get("error") {
        return Err(format!("API error ({}): {}",
            model_id, err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown")));
    }

    Ok(resp["choices"][0]["message"]["content"]
        .as_str().unwrap_or("(no response)").to_string())
}

// ─────────────────────────────────────────
// STREAMING CALL
// ─────────────────────────────────────────

pub async fn stream_model(
    app: &AppHandle,
    client: &Client,
    auth: &AuthState,
    room_id: &str,
    model_id: &str,
    messages: &[ChatMessage],
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<String, String> {
    // Non-streaming fallback for ERIN and Ollama
    if model_id == "erin" || model_id.starts_with("erin-") {
        let content = call_erin(client, auth, messages).await?;
        app.emit("stream-start", json!({"room_id": room_id, "speaker": model_id})).ok();
        // Simulate streaming for consistent UX
        for chunk in content.split_whitespace() {
            app.emit("stream-delta", json!({"room_id": room_id, "speaker": model_id, "delta": format!("{} ", chunk)})).ok();
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }
        app.emit("stream-end", json!({"room_id": room_id, "speaker": model_id, "content": content.clone(), "cancelled": false})).ok();
        return Ok(content);
    }

    if model_id.starts_with("ollama/") || model_id == "ollama" {
        let content = call_ollama(client, auth, model_id, messages).await?;
        app.emit("stream-start", json!({"room_id": room_id, "speaker": model_id})).ok();
        app.emit("stream-delta", json!({"room_id": room_id, "speaker": model_id, "delta": content.clone()})).ok();
        app.emit("stream-end", json!({"room_id": room_id, "speaker": model_id, "content": content.clone(), "cancelled": false})).ok();
        return Ok(content);
    }

    if model_id == "claude-web" {
        let content = call_claude_web(client, auth, messages).await?;
        app.emit("stream-start", json!({"room_id": room_id, "speaker": model_id})).ok();
        app.emit("stream-delta", json!({"room_id": room_id, "speaker": model_id, "delta": content.clone()})).ok();
        app.emit("stream-end", json!({"room_id": room_id, "speaker": model_id, "content": content.clone(), "cancelled": false})).ok();
        return Ok(content);
    }

    if model_id == "grok-web" {
        let content = call_grok_web(client, auth, messages).await?;
        app.emit("stream-start", json!({"room_id": room_id, "speaker": model_id})).ok();
        app.emit("stream-delta", json!({"room_id": room_id, "speaker": model_id, "delta": content.clone()})).ok();
        app.emit("stream-end", json!({"room_id": room_id, "speaker": model_id, "content": content.clone(), "cancelled": false})).ok();
        return Ok(content);
    }

    let (provider, api_base) = route_model(model_id);
    let auth_hdr = build_auth_header(&provider, auth).await?;

    let api_msgs: Vec<serde_json::Value> = messages.iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();

    let mut req = client
        .post(format!("{}/chat/completions", api_base))
        .json(&json!({
            "model": model_id,
            "messages": api_msgs,
            "stream": true,
            "max_tokens": 1000
        }));

    if let Some((hdr, val)) = auth_hdr { req = req.header(hdr, val); }
    for (k, v) in extra_headers(&provider) { req = req.header(k, v); }

    let response = req.send().await
        .map_err(|e| format!("Stream request failed ({}): {}", model_id, e))?;

    app.emit("stream-start", json!({"room_id": room_id, "speaker": model_id})).ok();

    let mut byte_stream = response.bytes_stream();
    let mut full_content = String::new();
    let mut buffer = String::new();
    let mut cancelled = false;

    loop {
        tokio::select! {
            _ = &mut cancel_rx => { cancelled = true; break; }
            chunk = byte_stream.next() => {
                match chunk {
                    None => break,
                    Some(Err(e)) => return Err(format!("Stream error: {}", e)),
                    Some(Ok(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();
                            if line.starts_with("data: ") {
                                let data = line[6..].trim();
                                if data == "[DONE]" { break; }
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                                        full_content.push_str(delta);
                                        app.emit("stream-delta", json!({
                                            "room_id": room_id, "speaker": model_id, "delta": delta
                                        })).ok();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    app.emit("stream-end", json!({
        "room_id": room_id, "speaker": model_id,
        "content": full_content.clone(), "cancelled": cancelled
    })).ok();

    Ok(full_content)
}

// ─────────────────────────────────────────
// CONNECTION TEST
// ─────────────────────────────────────────

pub async fn test_provider(
    client: &Client,
    auth: &AuthState,
    provider_id: &str,
) -> Result<String, String> {
    match provider_id {
        "erin" => {
            let base = auth.get_key("erin-endpoint").await
                .unwrap_or_else(|| "http://10.1.1.19:5000".into());
            let resp = client.post(format!("{}/api/chat", base))
                .timeout(std::time::Duration::from_secs(8))
                .json(&json!({"message": "ping", "conversation_id": "test"}))
                .send().await.map_err(|e| format!("ERIN unreachable: {}", e))?;
            if resp.status().is_success() {
                Ok(format!("✅ ERIN responding (HTTP {})", resp.status().as_u16()))
            } else {
                Err(format!("ERIN returned HTTP {}", resp.status()))
            }
        }
        "ollama" => {
            let base = auth.get_key("ollama-endpoint").await
                .unwrap_or_else(|| "http://localhost:11434".into());
            let resp = client.get(format!("{}/api/tags", base))
                .timeout(std::time::Duration::from_secs(5))
                .send().await.map_err(|e| format!("Ollama unreachable at {}: {}", base, e))?;
            if resp.status().is_success() {
                let data: serde_json::Value = resp.json().await.unwrap_or_default();
                let count = data["models"].as_array().map(|m| m.len()).unwrap_or(0);
                Ok(format!("✅ Ollama running — {} models loaded", count))
            } else {
                Err(format!("Ollama returned HTTP {}", resp.status()))
            }
        }
        "anthropic-web" => {
            let has_key = auth.get_key("anthropic-web").await.is_some();
            if has_key { Ok("✅ Claude session imported".into()) }
            else { Err("No session — use Import from Claude Desktop".into()) }
        }
        "xai-web" => {
            let has_key = auth.get_key("xai-web").await.is_some();
            if has_key { Ok("✅ Grok session imported".into()) }
            else { Err("No session — use Import from Grok Desktop".into()) }
        }
        _ => {
            let (provider, api_base) = match provider_id {
                "anthropic"      => (Provider::Anthropic,     "https://api.anthropic.com/v1"),
                "xai"            => (Provider::XAI,           "https://api.x.ai/v1"),
                "google"         => (Provider::Gemini,        "https://generativelanguage.googleapis.com/v1beta/openai"),
                "openai"         => (Provider::OpenAI,        "https://api.openai.com/v1"),
                "github-copilot" => (Provider::GitHubCopilot, "https://api.githubcopilot.com"),
                "mistral"        => (Provider::Mistral,       "https://api.mistral.ai/v1"),
                _ => return Err(format!("Unknown provider: {}", provider_id)),
            };

            let auth_hdr = build_auth_header(&provider, auth).await?;
            let mut req = client.get(format!("{}/models", api_base))
                .timeout(std::time::Duration::from_secs(8));
            if let Some((hdr, val)) = auth_hdr { req = req.header(hdr, val); }
            for (k, v) in extra_headers(&provider) { req = req.header(k, v); }

            let resp = req.send().await.map_err(|e| format!("Connection failed: {}", e))?;
            match resp.status().as_u16() {
                200..=299 => Ok(format!("✅ Connected (HTTP {})", resp.status().as_u16())),
                401 => Err("❌ Invalid API key (401 Unauthorized)".into()),
                403 => Err("❌ Access denied (403 Forbidden)".into()),
                n   => Err(format!("⚠️ HTTP {} — check key and endpoint", n)),
            }
        }
    }
}
