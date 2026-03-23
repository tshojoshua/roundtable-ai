//! auth.rs — Provider configuration, API key management, secure storage

use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, State};
use tokio::sync::Mutex;

// ─────────────────────────────────────────
// PROVIDER REGISTRY
// ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub label: String,
    pub api_base: String,
    pub key_console_url: String,
    pub key_placeholder: String,
    pub key_header: String,
    pub key_prefix: String,
    pub session_login_url: Option<String>,
    pub note: String,
    pub auth_type: String, // "apikey" | "session" | "none" | "custom"
}

pub fn provider_registry() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: "anthropic".into(),
            label: "Claude (API Key)".into(),
            api_base: "https://api.anthropic.com/v1".into(),
            key_console_url: "https://console.anthropic.com/settings/keys".into(),
            key_placeholder: "sk-ant-api03-...".into(),
            key_header: "x-api-key".into(),
            key_prefix: "".into(),
            session_login_url: None,
            note: "Official API key from Anthropic Console. Pay-per-token, separate from Claude Pro.".into(),
            auth_type: "apikey".into(),
        },
        ProviderConfig {
            id: "anthropic-web".into(),
            label: "Claude (Desktop Session)".into(),
            api_base: "https://claude.ai".into(),
            key_console_url: "".into(),
            key_placeholder: "Auto-imported from Claude Desktop".into(),
            key_header: "Cookie".into(),
            key_prefix: "sessionKey=".into(),
            session_login_url: None,
            note: "Uses your Claude Desktop session — no extra cost. Click 'Import from Claude Desktop' above.".into(),
            auth_type: "session".into(),
        },
        ProviderConfig {
            id: "xai".into(),
            label: "Grok (API Key)".into(),
            api_base: "https://api.x.ai/v1".into(),
            key_console_url: "https://console.x.ai/".into(),
            key_placeholder: "xai-...".into(),
            key_header: "Authorization".into(),
            key_prefix: "Bearer ".into(),
            session_login_url: None,
            note: "Official xAI API key from console.x.ai. Separate from SuperGrok subscription.".into(),
            auth_type: "apikey".into(),
        },
        ProviderConfig {
            id: "xai-web".into(),
            label: "Grok (Desktop Session)".into(),
            api_base: "https://grok.com".into(),
            key_console_url: "".into(),
            key_placeholder: "Auto-imported from Grok Desktop".into(),
            key_header: "Cookie".into(),
            key_prefix: "".into(),
            session_login_url: None,
            note: "Uses your Grok Desktop session — uses your SuperGrok subscription. Click 'Import from Grok Desktop' above.".into(),
            auth_type: "session".into(),
        },
        ProviderConfig {
            id: "google".into(),
            label: "Gemini (Google AI Studio)".into(),
            api_base: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
            key_console_url: "https://aistudio.google.com/app/apikey".into(),
            key_placeholder: "AIzaSy...".into(),
            key_header: "Authorization".into(),
            key_prefix: "Bearer ".into(),
            session_login_url: None,
            note: "Free API key from Google AI Studio. Gemini 2.5 Pro available. Gemini Advanced subscription is separate.".into(),
            auth_type: "apikey".into(),
        },
        ProviderConfig {
            id: "openai".into(),
            label: "OpenAI (GPT-4o)".into(),
            api_base: "https://api.openai.com/v1".into(),
            key_console_url: "https://platform.openai.com/api-keys".into(),
            key_placeholder: "sk-proj-...".into(),
            key_header: "Authorization".into(),
            key_prefix: "Bearer ".into(),
            session_login_url: None,
            note: "Official OpenAI API key. ChatGPT Plus subscription is separate from API access.".into(),
            auth_type: "apikey".into(),
        },
        ProviderConfig {
            id: "github-copilot".into(),
            label: "GitHub Copilot".into(),
            api_base: "https://api.githubcopilot.com".into(),
            key_console_url: "https://github.com/settings/tokens/new?scopes=copilot".into(),
            key_placeholder: "github_pat_...".into(),
            key_header: "Authorization".into(),
            key_prefix: "Bearer ".into(),
            session_login_url: None,
            note: "Requires active GitHub Copilot subscription. Create a Personal Access Token with 'copilot' scope. Gives access to GPT-4o, Claude 3.5, and more through your subscription.".into(),
            auth_type: "apikey".into(),
        },
        ProviderConfig {
            id: "mistral".into(),
            label: "Mistral AI".into(),
            api_base: "https://api.mistral.ai/v1".into(),
            key_console_url: "https://console.mistral.ai/api-keys".into(),
            key_placeholder: "...".into(),
            key_header: "Authorization".into(),
            key_prefix: "Bearer ".into(),
            session_login_url: None,
            note: "API key from console.mistral.ai. La Plateforme pay-as-you-go.".into(),
            auth_type: "apikey".into(),
        },
        ProviderConfig {
            id: "erin".into(),
            label: "ERIN (Local AGI)".into(),
            api_base: "http://10.1.1.19:5000".into(),
            key_console_url: "".into(),
            key_placeholder: "".into(),
            key_header: "".into(),
            key_prefix: "".into(),
            session_login_url: None,
            note: "Your local AGI running Qwen2.5-Omni-7B on Transformers at 10.1.1.19:5000. No auth required. Endpoint: /api/chat".into(),
            auth_type: "none".into(),
        },
        ProviderConfig {
            id: "ollama".into(),
            label: "Ollama (Configurable)".into(),
            api_base: "http://localhost:11434".into(),
            key_console_url: "".into(),
            key_placeholder: "".into(),
            key_header: "".into(),
            key_prefix: "".into(),
            session_login_url: None,
            note: "Local Ollama instance. Configure the endpoint and model in settings below.".into(),
            auth_type: "none".into(),
        },
    ]
}

// ─────────────────────────────────────────
// SECURE VAULT
// ─────────────────────────────────────────

const KEYRING_SERVICE: &str = "roundtable-ai";

fn keyring_set(key: &str, secret: &str) -> Result<(), String> {
    Entry::new(KEYRING_SERVICE, key)
        .map_err(|e| format!("Keyring init: {}", e))?
        .set_password(secret)
        .map_err(|e| format!("Keyring write: {}", e))
}

fn keyring_get(key: &str) -> Option<String> {
    Entry::new(KEYRING_SERVICE, key).ok()?.get_password().ok()
}

fn keyring_delete(key: &str) -> Result<(), String> {
    Entry::new(KEYRING_SERVICE, key)
        .map_err(|e| e.to_string())?
        .delete_credential()
        .map_err(|e| e.to_string())
}

fn fallback_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home)
        .join(".config").join("roundtable-ai").join("keys.json")
}

// ─────────────────────────────────────────
// AUTH STATE
// ─────────────────────────────────────────

pub struct AuthState {
    pub cache: Mutex<HashMap<String, String>>,
}

impl AuthState {
    pub fn new() -> Self {
        Self { cache: Mutex::new(HashMap::new()) }
    }

    pub async fn get_key(&self, id: &str) -> Option<String> {
        {
            let cache = self.cache.lock().await;
            if let Some(v) = cache.get(id) { return Some(v.clone()); }
        }
        if let Some(v) = keyring_get(id) {
            self.cache.lock().await.insert(id.to_string(), v.clone());
            return Some(v);
        }
        // file fallback
        let path = fallback_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&data) {
                if let Some(v) = map.get(id) {
                    self.cache.lock().await.insert(id.to_string(), v.clone());
                    return Some(v.clone());
                }
            }
        }
        None
    }

    pub async fn set_key(&self, id: &str, value: &str) -> Result<String, String> {
        let storage = match keyring_set(id, value) {
            Ok(_) => "keyring",
            Err(_) => {
                self.file_set(id, value)?;
                "fallback"
            }
        };
        self.cache.lock().await.insert(id.to_string(), value.to_string());
        Ok(storage.into())
    }

    fn file_set(&self, id: &str, value: &str) -> Result<(), String> {
        let path = fallback_path();
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut map: HashMap<String, String> = std::fs::read_to_string(&path)
            .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        map.insert(id.to_string(), value.to_string());
        let data = serde_json::to_string(&map).map_err(|e| e.to_string())?;
        std::fs::write(&path, &data).map_err(|e| e.to_string())?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub async fn delete_key(&self, id: &str) -> Result<(), String> {
        let _ = keyring_delete(id);
        let path = fallback_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            let mut map: HashMap<String, String> = serde_json::from_str(&data).unwrap_or_default();
            map.remove(id);
            let _ = std::fs::write(&path, serde_json::to_string(&map).unwrap_or_default());
        }
        self.cache.lock().await.remove(id);
        Ok(())
    }

    pub async fn get_storage_type(&self, id: &str) -> &'static str {
        if keyring_get(id).is_some() { return "keyring"; }
        let path = fallback_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            let map: HashMap<String, String> = serde_json::from_str(&data).unwrap_or_default();
            if map.contains_key(id) { return "fallback"; }
        }
        "none"
    }
}

// ─────────────────────────────────────────
// AICHAT CONFIG WRITER (optional, for aichat sidecar compat)
// ─────────────────────────────────────────

pub fn write_aichat_config(provider_id: &str, api_key: &str) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let config_dir = std::path::PathBuf::from(&home).join(".config").join("aichat");
    std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let config_path = config_dir.join("config.yaml");
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    let marker = format!("# roundtable-managed-{}", provider_id);

    let block = match provider_id {
        "anthropic" => format!("{marker}\n- type: openai-compatible\n  name: claude\n  api_base: https://api.anthropic.com/v1\n  api_key: {api_key}\n"),
        "xai"       => format!("{marker}\n- type: openai-compatible\n  name: grok\n  api_base: https://api.x.ai/v1\n  api_key: {api_key}\n"),
        "google"    => format!("{marker}\n- type: openai-compatible\n  name: gemini\n  api_base: https://generativelanguage.googleapis.com/v1beta/openai\n  api_key: {api_key}\n"),
        "openai"    => format!("{marker}\n- type: openai\n  api_key: {api_key}\n"),
        "mistral"   => format!("{marker}\n- type: openai-compatible\n  name: mistral\n  api_base: https://api.mistral.ai/v1\n  api_key: {api_key}\n"),
        _ => return Ok(()),
    };

    let new_config = if existing.contains(&marker) {
        existing.clone()
    } else {
        let base = if existing.is_empty() { "clients:\n".to_string() }
                   else if !existing.contains("clients:") { format!("{}\nclients:\n", existing.trim_end()) }
                   else { existing.clone() };
        base.replacen("clients:\n", &format!("clients:\n{}", block), 1)
    };
    std::fs::write(&config_path, new_config).map_err(|e| e.to_string())
}

// ─────────────────────────────────────────
// TAURI COMMANDS
// ─────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AuthStatus {
    pub provider_id: String,
    pub label: String,
    pub has_key: bool,
    pub storage: String,
    pub note: String,
    pub key_console_url: String,
    pub has_session_login: bool,
    pub auth_type: String,
}

#[tauri::command]
pub async fn list_providers() -> Result<Vec<ProviderConfig>, String> {
    Ok(provider_registry())
}

#[tauri::command]
pub async fn get_auth_status(auth: State<'_, AuthState>) -> Result<Vec<AuthStatus>, String> {
    let mut result = Vec::new();
    for p in provider_registry() {
        let storage = auth.get_storage_type(&p.id).await;
        result.push(AuthStatus {
            provider_id: p.id.clone(),
            label: p.label.clone(),
            has_key: storage != "none",
            storage: storage.into(),
            note: p.note.clone(),
            key_console_url: p.key_console_url.clone(),
            has_session_login: p.session_login_url.is_some(),
            auth_type: p.auth_type.clone(),
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn save_api_key(
    auth: State<'_, AuthState>,
    provider_id: String,
    api_key: String,
) -> Result<serde_json::Value, String> {
    if api_key.trim().is_empty() { return Err("API key cannot be empty".into()); }
    let storage = auth.set_key(&provider_id, api_key.trim()).await?;
    write_aichat_config(&provider_id, api_key.trim()).ok();
    Ok(serde_json::json!({ "storage": storage, "provider": provider_id }))
}

#[tauri::command]
pub async fn save_config_value(
    auth: State<'_, AuthState>,
    key: String,
    value: String,
) -> Result<(), String> {
    auth.set_key(&key, &value).await?;
    Ok(())
}

#[tauri::command]
pub async fn get_config_value(
    auth: State<'_, AuthState>,
    key: String,
) -> Result<Option<String>, String> {
    Ok(auth.get_key(&key).await)
}

#[tauri::command]
pub async fn delete_api_key(auth: State<'_, AuthState>, provider_id: String) -> Result<(), String> {
    auth.delete_key(&provider_id).await
}

#[tauri::command]
pub async fn test_connection(
    app: AppHandle,
    auth: State<'_, AuthState>,
    provider_id: String,
) -> Result<String, String> {
    let client = Client::new();
    crate::connector::test_provider(&client, &auth, &provider_id).await
}

#[tauri::command]
pub async fn open_console(_app: AppHandle, provider_id: String) -> Result<(), String> {
    let registry = provider_registry();
    let cfg = registry.iter().find(|p| p.id == provider_id)
        .ok_or_else(|| format!("Unknown provider: {}", provider_id))?;
    if cfg.key_console_url.is_empty() { return Err("No console URL for this provider".into()); }
    open::that(&cfg.key_console_url).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_login_window(app: AppHandle, provider_id: String) -> Result<(), String> {
    let registry = provider_registry();
    let cfg = registry.iter().find(|p| p.id == provider_id)
        .ok_or_else(|| format!("Unknown provider: {}", provider_id))?;
    let login_url = cfg.session_login_url.as_ref()
        .ok_or_else(|| format!("{} does not support session login", cfg.label))?;
    let label = format!("login-{}-{}", provider_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs());
    let url = tauri::WebviewUrl::External(login_url.parse().map_err(|e: url::ParseError| e.to_string())?);
    tauri::WebviewWindowBuilder::new(&app, label, url)
        .title(format!("Login — {}", cfg.label))
        .inner_size(900.0, 700.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub async fn get_auth_headers(auth: &AuthState, provider_id: &str) -> Vec<(String, String)> {
    let registry = provider_registry();
    let Some(cfg) = registry.iter().find(|p| p.id == provider_id) else { return vec![]; };
    if cfg.key_header.is_empty() { return vec![]; }
    if let Some(key) = auth.get_key(provider_id).await {
        return vec![(cfg.key_header.clone(), format!("{}{}", cfg.key_prefix, key))];
    }
    vec![]
}
