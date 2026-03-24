use crate::auth::AuthState;
use crate::connector;
use chrono::Utc;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::ShellExt;
use tokio::sync::{oneshot, Mutex};

// ─────────────────────────────────────────
// TYPES
// ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub speaker: Option<String>,
    pub ts: Option<i64>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into(), speaker: None, ts: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into(), speaker: Some("user".into()), ts: Some(Utc::now().timestamp_millis()) }
    }
    pub fn assistant(content: impl Into<String>, speaker: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into(), speaker: Some(speaker.into()), ts: Some(Utc::now().timestamp_millis()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ModeratorAction {
    LetModelSpeak,
    RequestToolUse,
    SummarizeDiscussion,
    GenerateMeetingMinutes,
    EndConference,
}

impl Default for ModeratorAction {
    fn default() -> Self { Self::LetModelSpeak }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModeratorDecision {
    pub action: ModeratorAction,
    pub target_model: Option<String>,
    pub reasoning: String,
    pub tool_call: Option<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceRoom {
    pub id: String,
    pub messages: Vec<Message>,
    pub participants: Vec<String>,
    pub moderator_model: String,
    pub mode: String,              // "moderator" | "parallel" | "roundrobin"
    pub turn_index: usize,
    pub is_generating: bool,
}

#[derive(Debug, Serialize)]
pub struct RoomSummary {
    pub id: String,
    pub mode: String,
    pub participants: Vec<String>,
    pub moderator_model: String,
    pub message_count: usize,
    pub is_generating: bool,
}

// ─────────────────────────────────────────
// ENGINE STATE
// ─────────────────────────────────────────

pub struct ConferenceEngine {
    pub rooms: HashMap<String, ConferenceRoom>,
    pub aichat_base: Option<String>,
    pub aichat_child: Option<tauri_plugin_shell::process::CommandChild>,
    pub client: Client,
    pub cancel_txs: HashMap<String, oneshot::Sender<()>>,
}

impl Default for ConferenceEngine {
    fn default() -> Self {
        Self {
            rooms: HashMap::new(),
            aichat_base: None,
            aichat_child: None,
            client: Client::new(),
            cancel_txs: HashMap::new(),
        }
    }
}

// ─────────────────────────────────────────
// SIDECAR
// ─────────────────────────────────────────

async fn ensure_aichat(app: &AppHandle, engine: &mut ConferenceEngine) -> Result<String, String> {
    if let Some(base) = &engine.aichat_base {
        return Ok(base.clone());
    }

    let (mut rx, child) = app
        .shell()
        .sidecar("aichat")
        .map_err(|e| e.to_string())?
        .args(["--serve", "127.0.0.1:0"])
        .spawn()
        .map_err(|e| e.to_string())?;

    engine.aichat_child = Some(child);

    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(15);
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err("Timeout waiting for aichat to start".into());
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        match tokio::time::timeout(tokio::time::Duration::from_millis(300), rx.recv()).await {
            Ok(Some(event)) => {
                use tauri_plugin_shell::process::CommandEvent;
                if let CommandEvent::Stdout(b) | CommandEvent::Stderr(b) = event {
                    let text = String::from_utf8_lossy(&b);
                    if let Some(start) = text.find("http://127.0.0.1:") {
                        let rest = &text[start..];
                        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                        let addr = rest[..end].trim().to_string();
                        println!("[Roundtable] aichat ready @ {}", addr);
                        engine.aichat_base = Some(addr.clone());
                        return Ok(addr);
                    }
                }
            }
            _ => continue,
        }
    }
}

// ─────────────────────────────────────────
// HTTP HELPERS
// ─────────────────────────────────────────

async fn call_model_simple(
    client: &Client,
    base: &str,
    model: &str,
    messages: &[Message],
    max_tokens: u32,
) -> Result<String, String> {
    let api_msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();

    let payload = json!({
        "model": model,
        "messages": api_msgs,
        "max_tokens": max_tokens,
        "stream": false
    });

    let resp: serde_json::Value = client
        .post(format!("{}/v1/chat/completions", base))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("HTTP error calling {}: {}", model, e))?
        .json()
        .await
        .map_err(|e| format!("JSON parse error for {}: {}", model, e))?;

    Ok(resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no response)")
        .to_string())
}

#[allow(dead_code)]
async fn call_model_with_tools(
    client: &Client,
    base: &str,
    model: &str,
    messages: &[Message],
) -> Result<String, String> {
    let api_msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();

    let tools = json!([
        {
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web for current information. Use when facts, recent news, or live data are needed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "The search query" }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "rag_lookup",
                "description": "Look up information in the loaded knowledge base documents.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to look up" }
                    },
                    "required": ["query"]
                }
            }
        }
    ]);

    let payload = json!({
        "model": model,
        "messages": api_msgs,
        "tools": tools,
        "tool_choice": "auto",
        "max_tokens": 500,
        "stream": false
    });

    let resp: serde_json::Value = client
        .post(format!("{}/v1/chat/completions", base))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    Ok(resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("(no response)")
        .to_string())
}

// ─────────────────────────────────────────
// MODERATOR LOGIC
// ─────────────────────────────────────────

fn render_history(messages: &[Message]) -> String {
    messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| format!("[{}]: {}", m.speaker.as_deref().unwrap_or(&m.role), m.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn get_moderator_decision(
    client: &Client,
    auth: &AuthState,
    room: &ConferenceRoom,
) -> Result<ModeratorDecision, String> {
    let system_prompt = format!(
        "You are ERIN, AGI moderator of a conference room.\n\
Participants: {}.\n\
Turn {}/∞.\n\n\
Your goals:\n\
1. Keep discussion productive and on-topic\n\
2. Give all participants fair turns\n\
3. Identify conflicts and resolve them\n\
4. Request tools (web_search, rag_lookup) when facts are needed\n\
5. Summarize when discussion loops or exceeds 8 turns\n\
6. Conclude when consensus is reached or after 12 turns\n\n\
Respond ONLY with valid JSON (no markdown, no explanation, raw JSON only):\n\
{{\"action\":\"LetModelSpeak\"|\"RequestToolUse\"|\"SummarizeDiscussion\"|\"GenerateMeetingMinutes\"|\"EndConference\",\
\"target_model\":null|\"<model_id>\",\"reasoning\":\"<1 sentence>\",\
\"tool_call\":null|{{\"name\":\"web_search\"|\"rag_lookup\",\"arguments\":{{\"query\":\"<query>\"}}}}}}",
        room.participants.join(", "),
        room.messages.len()
    );

    let history = render_history(&room.messages);
    let msgs = vec![
        Message::system(&system_prompt),
        Message::user(format!("Current discussion:\n\n{}\n\nYour decision:", history)),
    ];

    let raw = connector::call_model(client, auth, &room.moderator_model, &to_chat_msgs(&msgs), 300).await?;

    // Strip markdown fences if present
    let clean = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // Try parse; on fail try to extract JSON block
    match serde_json::from_str::<ModeratorDecision>(clean) {
        Ok(d) => Ok(d),
        Err(_) => {
            // Try extracting JSON block
            if let Some(start) = clean.find('{') {
                if let Some(end) = clean.rfind('}') {
                    if let Ok(d) = serde_json::from_str::<ModeratorDecision>(&clean[start..=end]) {
                        return Ok(d);
                    }
                }
            }
            // Repair prompt
            let repair_msgs = vec![
                Message::system("You output invalid JSON. Respond with ONLY the corrected JSON object, nothing else."),
                Message::user(format!("Fix this invalid JSON response:
{}", raw)),
            ];
            let repaired = connector::call_model(client, auth, &room.moderator_model, &to_chat_msgs(&repair_msgs), 300).await
                .unwrap_or_default();
            let clean2 = repaired.trim()
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();
            Ok(serde_json::from_str(clean2).unwrap_or(ModeratorDecision {
                action: ModeratorAction::LetModelSpeak,
                target_model: room.participants.first().cloned(),
                reasoning: "JSON repair fallback".into(),
                tool_call: None,
            }))
        }
    }
}

async fn execute_tool(name: &str, args: &serde_json::Value) -> String {
    // Real tool execution — for now web_search returns a note; wire to real search later
    match name {
        "web_search" => {
            let query = args["query"].as_str().unwrap_or("unknown");
            format!("🔍 Web search for '{}': [Tool execution pending — wire to real search API]", query)
        }
        "rag_lookup" => {
            let query = args["query"].as_str().unwrap_or("unknown");
            format!("📚 RAG lookup for '{}': [Tool execution pending — wire to knowledge base]", query)
        }
        _ => format!("Unknown tool: {}", name),
    }
}


/// Convert internal Message list to connector ChatMessage list (strips speaker/ts)
fn to_chat_msgs(messages: &[Message]) -> Vec<connector::ChatMessage> {
    messages.iter().map(|m| connector::ChatMessage {
        role: m.role.clone(),
        content: m.content.clone(),
    }).collect()
}

/// Build messages with a model-specific system prompt prepended
fn with_system_prompt(model_id: &str, messages: &[Message]) -> Vec<connector::ChatMessage> {
    let system = match model_id {
        m if m.starts_with("claude") || m == "claude-web" => {
            "You are Claude, an AI assistant made by Anthropic, participating in a Roundtable AI conference. Other AI models are also present. Engage thoughtfully with the topic, building on or respectfully challenging other perspectives. Be direct and substantive. Do not introduce yourself unless asked."
        }
        m if m.starts_with("grok") || m == "grok-web" => {
            "You are Grok, an AI made by xAI, participating in a Roundtable AI conference. Other AI models are present. Be direct, intellectually honest, and willing to challenge assumptions. Engage with the topic — don't just summarize, add your own perspective."
        }
        m if m.starts_with("gemini") => {
            "You are Gemini, an AI made by Google DeepMind, participating in a Roundtable AI conference. Bring analytical depth and breadth of knowledge. Engage with other perspectives in the conversation. Be concise and substantive."
        }
        m if m.starts_with("gpt") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("github/gpt") => {
            "You are GPT-4o, an AI made by OpenAI, participating in a Roundtable AI conference. Engage analytically with the topic. Be helpful, direct, and add genuine insight. Other AI models are present — engage with their perspectives where relevant."
        }
        m if m.starts_with("mistral") => {
            "You are Mistral, an AI participating in a Roundtable AI conference. Be precise, efficient, and direct. Add your perspective to the discussion."
        }
        m if m.starts_with("github/claude") => {
            "You are Claude (via GitHub Copilot), participating in a Roundtable AI conference. Engage thoughtfully and be direct and substantive."
        }
        _ => "" // ERIN and Ollama: no injection, they have their own system prompts
    };

    let mut result = Vec::new();
    if !system.is_empty() {
        result.push(connector::ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });
    }
    for m in messages {
        result.push(connector::ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }
    result
}

// ─────────────────────────────────────────
// PERSISTENCE
// ─────────────────────────────────────────

fn rooms_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join("Documents").join("Roundtable")
}

fn rooms_file() -> PathBuf {
    rooms_path().join("rooms.json")
}

pub fn load_rooms_from_disk() -> HashMap<String, ConferenceRoom> {
    let path = rooms_file();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(rooms) = serde_json::from_str(&data) {
            println!("[Roundtable] Loaded {} rooms from disk", {
                let r: &HashMap<String, ConferenceRoom> = &rooms;
                r.len()
            });
            return rooms;
        }
    }
    HashMap::new()
}

pub fn save_rooms_to_disk(rooms: &HashMap<String, ConferenceRoom>) -> Result<(), String> {
    let dir = rooms_path();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let data = serde_json::to_string_pretty(rooms).map_err(|e| e.to_string())?;
    std::fs::write(rooms_file(), data).map_err(|e| e.to_string())?;
    Ok(())
}

async fn generate_minutes_content(client: &Client, auth: &AuthState, room: &ConferenceRoom) -> Result<String, String> {
    let history = render_history(&room.messages);
    let prompt = format!(
        "You are a professional meeting secretary.\n\
Generate clean, structured Markdown meeting minutes from this AI conference transcript.\n\
Include: Title, Date/Time, Participants, Key Discussion Points, Agreements, Open Questions, Action Items.\n\n\
Transcript:\n{}",
        history
    );
    let msgs = vec![
        Message::system(prompt),
        Message::user("Generate meeting minutes now."),
    ];
    connector::call_model(client, auth, &room.moderator_model, &to_chat_msgs(&msgs), 2000).await
}

// ─────────────────────────────────────────
// TAURI COMMANDS — ROOM MANAGEMENT
// ─────────────────────────────────────────

#[tauri::command]
pub async fn create_room(
    app: AppHandle,
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
    participants: Vec<String>,
    moderator_model: String,
    mode: String,
) -> Result<(), String> {
    let mut engine = engine.lock().await;
    // No aichat needed — connector.rs calls providers directly

    engine.rooms.insert(room_id.clone(), ConferenceRoom {
        id: room_id.clone(),
        messages: vec![],
        participants: participants.clone(),
        moderator_model: moderator_model.clone(),
        mode: mode.clone(),
        turn_index: 0,
        is_generating: false,
    });

    save_rooms_to_disk(&engine.rooms).ok();

    app.emit("room-created", json!({
        "room_id": room_id,
        "participants": participants,
        "moderator": moderator_model,
        "mode": mode
    })).ok();

    println!("[Roundtable] Room '{}' created (mode={}, mod={})", room_id, mode, moderator_model);
    Ok(())
}

#[tauri::command]
pub async fn list_rooms(
    engine: State<'_, Mutex<ConferenceEngine>>,
) -> Result<Vec<RoomSummary>, String> {
    let engine = engine.lock().await;
    Ok(engine.rooms.values().map(|r| RoomSummary {
        id: r.id.clone(),
        mode: r.mode.clone(),
        participants: r.participants.clone(),
        moderator_model: r.moderator_model.clone(),
        message_count: r.messages.len(),
        is_generating: r.is_generating,
    }).collect())
}

#[tauri::command]
pub async fn delete_room(
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
) -> Result<(), String> {
    let mut engine = engine.lock().await;
    engine.rooms.remove(&room_id).ok_or("Room not found")?;
    save_rooms_to_disk(&engine.rooms).ok();
    Ok(())
}

#[tauri::command]
pub async fn set_room_mode(
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
    mode: String,
) -> Result<(), String> {
    let mut engine = engine.lock().await;
    let room = engine.rooms.get_mut(&room_id).ok_or("Room not found")?;
    if !["moderator", "parallel", "roundrobin"].contains(&mode.as_str()) {
        return Err(format!("Invalid mode: {}", mode));
    }
    room.mode = mode;
    Ok(())
}

#[tauri::command]
pub async fn get_room_messages(
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
) -> Result<Vec<Message>, String> {
    let engine = engine.lock().await;
    Ok(engine.rooms.get(&room_id).ok_or("Room not found")?.messages.clone())
}

#[tauri::command]
pub async fn clear_room(
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
) -> Result<(), String> {
    let mut engine = engine.lock().await;
    let room = engine.rooms.get_mut(&room_id).ok_or("Room not found")?;
    room.messages.clear();
    room.turn_index = 0;
    save_rooms_to_disk(&engine.rooms).ok();
    Ok(())
}

// ─────────────────────────────────────────
// TAURI COMMANDS — MESSAGING
// ─────────────────────────────────────────

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
    content: String,
) -> Result<(), String> {
    // Add user message
    {
        let mut engine = engine.lock().await;
        let room = engine.rooms.get_mut(&room_id).ok_or("Room not found")?;
        if room.is_generating {
            return Err("Room is currently generating".into());
        }
        room.messages.push(Message::user(&content));
        room.is_generating = true;
        save_rooms_to_disk(&engine.rooms).ok();
    }

    app.emit("message-added", json!({
        "room_id": room_id,
        "speaker": "user",
        "content": content
    })).ok();

    // Spawn turn using app handle to re-acquire state (avoids borrow escape)
    let app_clone = app.clone();
    let room_id_clone = room_id.clone();

    tokio::spawn(async move {
        let engine_state: tauri::State<Mutex<ConferenceEngine>> = app_clone.state();
        let auth_state: tauri::State<AuthState> = app_clone.state();
        if let Err(e) = run_turn(&app_clone, &engine_state, &auth_state, &room_id_clone).await {
            eprintln!("[Roundtable] Turn error: {}", e);
            app_clone.emit("turn-error", json!({
                "room_id": room_id_clone,
                "error": e
            })).ok();
        }
        // Mark done
        let mut eng = engine_state.lock().await;
        if let Some(room) = eng.rooms.get_mut(&room_id_clone) {
            room.is_generating = false;
        }
        save_rooms_to_disk(&eng.rooms).ok();
        app_clone.emit("turn-complete", json!({ "room_id": room_id_clone })).ok();
    });

    Ok(())
}

async fn run_turn(
    app: &AppHandle,
    engine: &Mutex<ConferenceEngine>,
    auth: &AuthState,
    room_id: &str,
) -> Result<(), String> {
    let (mode, participants, moderator_model, messages_snap) = {
        let eng = engine.lock().await;
        let room = eng.rooms.get(room_id).ok_or("Room not found")?;
        (
            room.mode.clone(),
            room.participants.clone(),
            room.moderator_model.clone(),
            room.messages.clone(),
        )
    };

    let client = {
        let eng = engine.lock().await;
        eng.client.clone()
    };

    match mode.as_str() {
        "parallel" => {
            let non_mod: Vec<String> = participants.iter()
                .filter(|p| **p != moderator_model)
                .cloned()
                .collect();

            let mut join_set = tokio::task::JoinSet::new();
            for model in non_mod {
                let app2 = app.clone();
                let client2 = client.clone();
                let msgs2 = with_system_prompt(&model, &messages_snap);
                let room2 = room_id.to_string();
                let model2 = model.clone();
                let app3 = app.clone();
                join_set.spawn(async move {
                    let auth3: tauri::State<AuthState> = app3.state();
                    let (_tx, rx) = oneshot::channel::<()>();
                    let content = connector::stream_model(&app3, &client2, &*auth3, &room2, &model2, &msgs2, rx)
                        .await.unwrap_or_else(|e| format!("(error: {})", e));
                    (model2, content)
                });
            }
            while let Some(Ok((model, content))) = join_set.join_next().await {
                let mut eng = engine.lock().await;
                if let Some(room) = eng.rooms.get_mut(room_id) {
                    if !content.is_empty() {
                        room.messages.push(Message::assistant(&content, &model));
                    }
                }
            }
        }

        "roundrobin" => {
            let turn_index = {
                let eng = engine.lock().await;
                eng.rooms.get(room_id).map(|r| r.turn_index).unwrap_or(0)
            };
            let non_mod: Vec<String> = participants.iter()
                .filter(|p| **p != moderator_model)
                .cloned()
                .collect();
            if non_mod.is_empty() { return Ok(()); }
            let model = non_mod[turn_index % non_mod.len()].clone();

            let (_tx, cancel_rx) = oneshot::channel::<()>();
            let auth_rr: tauri::State<AuthState> = app.state();
            let content = connector::stream_model(app, &client, &*auth_rr, room_id, &model, &with_system_prompt(&model, &messages_snap), cancel_rx)
                .await.unwrap_or_else(|e| format!("(error: {})", e));

            let mut eng = engine.lock().await;
            if let Some(room) = eng.rooms.get_mut(room_id) {
                if !content.is_empty() { room.messages.push(Message::assistant(&content, &model)); }
                room.turn_index += 1;
            }
        }

        _ => {
            // Moderator mode
            let non_mod: Vec<String> = participants.iter()
                .filter(|p| **p != moderator_model)
                .cloned()
                .collect();

            // Step 1: non-moderators speak in parallel
            let mut join_set = tokio::task::JoinSet::new();
            for model in &non_mod {
                let app2 = app.clone();
                let client2 = client.clone();
                let msgs2 = to_chat_msgs(&messages_snap);
                let room2 = room_id.to_string();
                let model2 = model.clone();
                join_set.spawn(async move {
                    let auth2: tauri::State<AuthState> = app2.state();
                    let (_tx, rx) = oneshot::channel::<()>();
                    let content = connector::stream_model(&app2, &client2, &*auth2, &room2, &model2, &msgs2, rx)
                        .await.unwrap_or_else(|e| format!("(error: {})", e));
                    (model2, content)
                });
            }
            while let Some(Ok((model, content))) = join_set.join_next().await {
                let mut eng = engine.lock().await;
                if let Some(room) = eng.rooms.get_mut(room_id) {
                    if !content.is_empty() { room.messages.push(Message::assistant(&content, &model)); }
                }
            }

            // Step 2: ERIN decides
            let room_snap = {
                let eng = engine.lock().await;
                eng.rooms.get(room_id).cloned().ok_or("Room not found")?
            };
            let auth_mod: tauri::State<AuthState> = app.state();
            let decision = get_moderator_decision(&client, &*auth_mod, &room_snap).await?;

            println!("[ERIN] {:?} | target={:?} | {}", decision.action, decision.target_model, decision.reasoning);

            app.emit("moderator-decision", serde_json::json!({
                "room_id": room_id,
                "decision": decision
            })).ok();

            // Step 3: Execute decision
            match &decision.action {
                ModeratorAction::LetModelSpeak => {
                    let target = decision.target_model.clone()
                        .unwrap_or_else(|| non_mod.first().cloned().unwrap_or_default());
                    if !target.is_empty() {
                        let msgs_raw = engine.lock().await.rooms.get(room_id).map(|r| r.messages.clone()).unwrap_or_default();
                        let msgs = with_system_prompt(&target, &msgs_raw);
                        let auth_ls: tauri::State<AuthState> = app.state();
                        let (_tx, rx) = oneshot::channel::<()>();
                        let content = connector::stream_model(app, &client, &*auth_ls, room_id, &target, &msgs, rx)
                            .await.unwrap_or_else(|e| format!("(error: {})", e));
                        let mut eng = engine.lock().await;
                        if let Some(room) = eng.rooms.get_mut(room_id) {
                            if !content.is_empty() { room.messages.push(Message::assistant(&content, &target)); }
                            room.messages.push(Message::assistant(format!("📋 *{}*", decision.reasoning), "ERIN [Moderator]"));
                        }
                    }
                }
                ModeratorAction::RequestToolUse => {
                    if let Some(tool) = &decision.tool_call {
                        let result = crate::conference::execute_tool(&tool.name, &tool.arguments).await;
                        app.emit("tool-called", serde_json::json!({"room_id": room_id, "tool": tool.name, "result": result})).ok();
                        let mut eng = engine.lock().await;
                        if let Some(room) = eng.rooms.get_mut(room_id) {
                            room.messages.push(Message { role: "tool".into(), content: result, speaker: Some(format!("🔧 {}", tool.name)), ts: Some(chrono::Utc::now().timestamp_millis()) });
                        }
                    }
                }
                ModeratorAction::SummarizeDiscussion => {
                    let msgs = to_chat_msgs(&engine.lock().await.rooms.get(room_id).map(|r| r.messages.clone()).unwrap_or_default());
                    let history = msgs.iter().map(|m| format!("[{}]: {}", m.role, m.content)).collect::<Vec<_>>().join("

");
                    let sum_msgs = vec![
                        connector::ChatMessage { role: "system".into(), content: "Summarize the key points, agreements, and open questions from this discussion in Markdown.".into() },
                        connector::ChatMessage { role: "user".into(), content: history },
                    ];
                    let auth_su: tauri::State<AuthState> = app.state();
                    let summary = connector::call_model(&client, &*auth_su, &moderator_model, &sum_msgs, 800)
                        .await.unwrap_or_else(|e| format!("(summary error: {})", e));
                    let mut eng = engine.lock().await;
                    if let Some(room) = eng.rooms.get_mut(room_id) {
                        room.messages.push(Message::assistant(format!("## 📊 Discussion Summary

{}", summary), "ERIN [Summary]"));
                    }
                }
                ModeratorAction::GenerateMeetingMinutes | ModeratorAction::EndConference => {
                    let room_snap2 = engine.lock().await.rooms.get(room_id).cloned().ok_or("Room not found")?;
                    let auth_gm: tauri::State<AuthState> = app.state();
                    let minutes = generate_minutes_content(&client, &*auth_gm, &room_snap2)
                        .await.unwrap_or_else(|e| format!("(minutes error: {})", e));
                    let dir = rooms_path();
                    std::fs::create_dir_all(&dir).ok();
                    let fname = format!("{}-minutes-{}.md", room_id, chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                    let fpath = dir.join(&fname);
                    std::fs::write(&fpath, &minutes).ok();
                    app.emit("minutes-ready", serde_json::json!({"room_id": room_id, "content": minutes.clone(), "path": fpath.to_string_lossy()})).ok();
                    let label = if matches!(decision.action, ModeratorAction::EndConference) { "ERIN [Minutes + Concluded]" } else { "ERIN [Minutes]" };
                    let mut eng = engine.lock().await;
                    if let Some(room) = eng.rooms.get_mut(room_id) {
                        room.messages.push(Message::assistant(format!("## 📝 Meeting Minutes

{}", minutes), label));
                    }
                }
            }
        }
    }

    Ok(())
}

// ─────────────────────────────────────────
// TAURI COMMANDS — CANCEL / EXPORT
// ─────────────────────────────────────────

#[tauri::command]
pub async fn cancel_generation(
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
) -> Result<(), String> {
    let mut engine = engine.lock().await;
    if let Some(tx) = engine.cancel_txs.remove(&room_id) {
        tx.send(()).ok();
    }
    if let Some(room) = engine.rooms.get_mut(&room_id) {
        room.is_generating = false;
    }
    Ok(())
}

#[tauri::command]
pub async fn export_minutes(
    app: AppHandle,
    engine: State<'_, Mutex<ConferenceEngine>>,
    room_id: String,
) -> Result<String, String> {
    let (client, room_snap) = {
        let eng = engine.lock().await;
        let room = eng.rooms.get(&room_id).ok_or("Room not found")?.clone();
        (eng.client.clone(), room)
    };
    let auth_state: tauri::State<AuthState> = app.state();
    let minutes = generate_minutes_content(&client, &auth_state, &room_snap).await?;

    let dir = rooms_path();
    std::fs::create_dir_all(&dir).ok();
    let fname = format!("{}-minutes-{}.md", room_id, Utc::now().format("%Y%m%d-%H%M%S"));
    let fpath = dir.join(&fname);
    std::fs::write(&fpath, &minutes).map_err(|e| e.to_string())?;

    app.emit("minutes-ready", json!({
        "room_id": room_id,
        "content": minutes.clone(),
        "path": fpath.to_string_lossy()
    })).ok();

    Ok(minutes)
}

#[tauri::command]
pub async fn save_rooms(engine: State<'_, Mutex<ConferenceEngine>>) -> Result<(), String> {
    let engine = engine.lock().await;
    save_rooms_to_disk(&engine.rooms)
}

#[tauri::command]
pub async fn get_aichat_status(
    engine: State<'_, Mutex<ConferenceEngine>>,
) -> Result<serde_json::Value, String> {
    let engine = engine.lock().await;
    Ok(json!({
        "running": engine.aichat_base.is_some(),
        "base_url": engine.aichat_base,
        "room_count": engine.rooms.len()
    }))
}
