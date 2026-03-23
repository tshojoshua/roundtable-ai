//! session_bridge.rs — Read session tokens from installed desktop apps
//!
//! Claude Desktop and Grok Desktop both use Electron/Chromium which stores
//! cookies in SQLite at known paths. We read them directly — same approach
//! the community Linux apps use.
//!
//! The cookies are encrypted with the OS keyring on Windows/macOS.
//! On Linux they use a static key ("peanuts") or GNOME keyring DPAPI.
//! We handle the Linux case (where your apps actually run).

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use tauri::State;
use crate::auth::AuthState;

// ─────────────────────────────────────────
// COOKIE PATHS
// ─────────────────────────────────────────

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

fn claude_cookie_path() -> PathBuf {
    home().join(".config/Claude/Cookies")
}

fn grok_cookie_path() -> PathBuf {
    home().join(".config/Grok-Desktop/Partitions/grok/Cookies")
}

// ─────────────────────────────────────────
// COOKIE READER
// ─────────────────────────────────────────

fn read_cookie(db_path: &PathBuf, host_pattern: &str, name: &str) -> Option<String> {
    if !db_path.exists() { return None; }

    // Open read-only (don't lock the db — app might be running)
    // Copy to temp first to avoid SQLITE_BUSY
    let tmp = std::env::temp_dir().join(format!(
        "roundtable_cookies_{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::copy(db_path, &tmp).ok()?;

    let conn = Connection::open_with_flags(&tmp, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;

    let query = "SELECT value, encrypted_value FROM cookies WHERE host_key LIKE ?1 AND name = ?2 LIMIT 1";
    let result = conn.query_row(query, rusqlite::params![host_pattern, name], |row| {
        let value: String = row.get(0)?;
        let encrypted: Vec<u8> = row.get(1)?;
        Ok((value, encrypted))
    });

    let _ = std::fs::remove_file(&tmp);

    match result {
        Ok((value, encrypted)) => {
            // If plaintext value exists, use it
            if !value.is_empty() {
                return Some(value);
            }
            // Otherwise decrypt (Linux: v10/v11 prefix + AES-128-CBC with "peanuts" key)
            decrypt_linux_cookie(&encrypted)
        }
        Err(_) => None,
    }
}

fn decrypt_linux_cookie(encrypted: &[u8]) -> Option<String> {
    // Chromium Linux cookie encryption:
    // v10/v11 prefix (3 bytes) + AES-128-CBC, key = PBKDF2("peanuts", salt, 1, 16)
    // Salt = "saltysalt", IV = " " * 16
    if encrypted.len() < 3 { return None; }

    let payload = if encrypted.starts_with(b"v10") || encrypted.starts_with(b"v11") {
        &encrypted[3..]
    } else {
        return None;
    };

    // Derive key using PBKDF2-HMAC-SHA1
    use std::num::NonZeroU32;
    let password = b"peanuts";
    let salt = b"saltysalt";
    let iterations = NonZeroU32::new(1).unwrap();
    let mut key = [0u8; 16];

    // Simple PBKDF2 implementation using hmac-sha1
    pbkdf2_hmac_sha1(password, salt, 1, &mut key);

    let iv = [0x20u8; 16]; // 16 spaces

    // AES-128-CBC decrypt
    aes_cbc_decrypt(payload, &key, &iv)
        .and_then(|plaintext| String::from_utf8(plaintext).ok())
        .map(|s| s.trim_matches('\0').to_string())
}

// Minimal PBKDF2-HMAC-SHA1 (1 iteration)
fn pbkdf2_hmac_sha1(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    // For 1 iteration this simplifies significantly
    // U1 = HMAC-SHA1(password, salt || block_index)
    use std::collections::hash_map::DefaultHasher;
    // We need a real crypto impl — use the ring or openssl crate
    // For now use openssl which is already a transitive dep
    let key = openssl::pkcs5::pbkdf2_hmac(
        password,
        salt,
        iterations as usize,
        openssl::hash::MessageDigest::sha1(),
        output,
    );
    // If openssl fails, output stays zeroed
    let _ = key;
}

fn aes_cbc_decrypt(ciphertext: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Option<Vec<u8>> {
    use openssl::symm::{decrypt, Cipher};
    decrypt(Cipher::aes_128_cbc(), key, Some(iv), ciphertext).ok()
}

// ─────────────────────────────────────────
// PUBLIC EXTRACTORS
// ─────────────────────────────────────────

pub fn extract_claude_session() -> Option<String> {
    read_cookie(&claude_cookie_path(), "%.claude.ai", "sessionKey")
}

pub fn extract_grok_tokens() -> Option<(String, String)> {
    let auth_token = read_cookie(&grok_cookie_path(), "%.x.com", "auth_token")?;
    let ct0 = read_cookie(&grok_cookie_path(), "%.x.com", "ct0")?;
    Some((auth_token, ct0))
}

pub fn extract_grok_sso() -> Option<String> {
    read_cookie(&grok_cookie_path(), "%.grok.com", "sso")
}

// ─────────────────────────────────────────
// TAURI COMMANDS
// ─────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct SessionImportResult {
    pub provider: String,
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn import_claude_session(
    auth: State<'_, AuthState>,
) -> Result<SessionImportResult, String> {
    match extract_claude_session() {
        Some(session_key) if !session_key.is_empty() => {
            // Store as the auth token for anthropic-web provider
            auth.set_key("anthropic-web", &session_key).await?;
            Ok(SessionImportResult {
                provider: "Claude".into(),
                success: true,
                message: format!("✅ Claude session imported (key: {}...{})",
                    &session_key[..8.min(session_key.len())],
                    &session_key[session_key.len().saturating_sub(4)..]),
            })
        }
        _ => Ok(SessionImportResult {
            provider: "Claude".into(),
            success: false,
            message: "❌ Claude Desktop not found or not logged in. Make sure Claude Desktop is installed and you're signed in.".into(),
        })
    }
}

#[tauri::command]
pub async fn import_grok_session(
    auth: State<'_, AuthState>,
) -> Result<SessionImportResult, String> {
    // Try auth_token + ct0 first
    if let Some((auth_token, ct0)) = extract_grok_tokens() {
        if !auth_token.is_empty() {
            // Store combined as "auth_token|ct0" — connector will split
            let combined = format!("{}|{}", auth_token, ct0);
            auth.set_key("xai-web", &combined).await?;
            return Ok(SessionImportResult {
                provider: "Grok".into(),
                success: true,
                message: format!("✅ Grok session imported from Grok Desktop"),
            });
        }
    }
    // Fallback: sso cookie
    if let Some(sso) = extract_grok_sso() {
        if !sso.is_empty() {
            auth.set_key("xai-web", &sso).await?;
            return Ok(SessionImportResult {
                provider: "Grok".into(),
                success: true,
                message: "✅ Grok SSO session imported".into(),
            });
        }
    }
    Ok(SessionImportResult {
        provider: "Grok".into(),
        success: false,
        message: "❌ Grok Desktop not found or not logged in. Make sure Grok Desktop is installed and you're signed in at grok.com.".into(),
    })
}

#[tauri::command]
pub async fn check_installed_apps() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "claude_desktop": claude_cookie_path().exists(),
        "grok_desktop": grok_cookie_path().exists(),
        "claude_logged_in": extract_claude_session().is_some(),
        "grok_logged_in": extract_grok_tokens().is_some() || extract_grok_sso().is_some(),
    }))
}
