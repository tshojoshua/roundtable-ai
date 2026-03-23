import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

// ── State ──
let rooms = []
let activeRoom = null
let messages = []
let streamBuffers = {}
let isGenerating = false

// ── DOM ──
const app = document.getElementById('app')
app.innerHTML = `
<style>
  :root { --bg: #030712; --bg2: #0f172a; --bg3: #1e293b; --border: #334155; --text: #f1f5f9; --muted: #64748b; --indigo: #6366f1; --indigo2: #4f46e5; --red: #ef4444; --green: #10b981; }
  * { box-sizing: border-box; }
  body { background: var(--bg); color: var(--text); font-family: system-ui,-apple-system,sans-serif; }
  #layout { display: flex; height: 100vh; overflow: hidden; }
  #nav { width: 56px; background: var(--bg2); border-right: 1px solid var(--border); display: flex; flex-direction: column; align-items: center; padding: 12px 0; gap: 8px; flex-shrink: 0; }
  #nav .logo { width: 36px; height: 36px; background: var(--indigo); border-radius: 10px; display: flex; align-items: center; justify-content: center; font-weight: 900; font-size: 14px; margin-bottom: 8px; }
  #nav a { width: 40px; height: 40px; border-radius: 10px; display: flex; align-items: center; justify-content: center; font-size: 20px; cursor: pointer; text-decoration: none; color: var(--text); transition: background 0.15s; }
  #nav a:hover { background: var(--bg3); }
  #nav a.active { background: var(--indigo); }
  #content { flex: 1; overflow: hidden; display: flex; flex-direction: column; }
  .page { display: none; flex: 1; flex-direction: column; overflow: hidden; }
  .page.active { display: flex; }

  /* Conference */
  #conf-inner { display: flex; height: 100%; overflow: hidden; }
  #rooms-panel { width: 200px; background: var(--bg2); border-right: 1px solid var(--border); display: flex; flex-direction: column; flex-shrink: 0; }
  #rooms-header { padding: 12px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); }
  #rooms-header span { font-weight: 600; font-size: 13px; }
  #add-room-btn { width: 24px; height: 24px; background: var(--indigo); border-radius: 50%; border: none; color: white; font-size: 18px; cursor: pointer; display: flex; align-items: center; justify-content: center; line-height: 1; }
  #new-room-form { padding: 8px; display: none; border-bottom: 1px solid var(--border); }
  #new-room-input { width: 100%; background: var(--bg3); border: 1px solid var(--border); border-radius: 8px; padding: 6px 8px; color: var(--text); font-size: 12px; outline: none; }
  #new-room-input:focus { border-color: var(--indigo); }
  #rooms-list { flex: 1; overflow-y: auto; padding: 4px; }
  .room-item { padding: 8px 10px; border-radius: 8px; cursor: pointer; font-size: 12px; color: var(--muted); margin: 2px 0; display: flex; align-items: center; justify-content: space-between; }
  .room-item:hover { background: var(--bg3); color: var(--text); }
  .room-item.active { background: var(--bg3); color: var(--text); }
  .room-item .room-del { opacity: 0; font-size: 10px; cursor: pointer; }
  .room-item:hover .room-del { opacity: 1; }
  #aichat-status { padding: 8px 12px; font-size: 11px; color: var(--muted); border-top: 1px solid var(--border); display: flex; align-items: center; gap: 6px; }
  #status-dot { width: 6px; height: 6px; border-radius: 50%; background: #f59e0b; }
  #chat-area { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
  #chat-header { padding: 10px 16px; background: var(--bg2); border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; }
  #chat-title { font-weight: 600; font-size: 14px; }
  #mode-btns { display: flex; gap: 4px; background: var(--bg3); border-radius: 8px; padding: 3px; }
  .mode-btn { padding: 4px 10px; border-radius: 6px; border: none; background: none; color: var(--muted); font-size: 11px; cursor: pointer; }
  .mode-btn.active { background: var(--indigo); color: white; }
  #chat-messages { flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 12px; }
  #empty-state { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px; color: var(--muted); text-align: center; padding: 40px; }
  #empty-state .big-icon { font-size: 64px; }
  #empty-state h2 { color: var(--text); font-size: 22px; }
  #empty-state p { font-size: 14px; line-height: 1.6; max-width: 360px; }
  #create-room-big { padding: 12px 24px; background: var(--indigo); color: white; border: none; border-radius: 12px; font-size: 15px; font-weight: 600; cursor: pointer; }
  #create-room-big:hover { background: var(--indigo2); }
  .msg-user { display: flex; justify-content: flex-end; }
  .msg-user .bubble { background: var(--indigo); color: white; padding: 10px 14px; border-radius: 18px 18px 4px 18px; max-width: 70%; font-size: 14px; line-height: 1.5; }
  .msg-ai { display: flex; gap: 10px; max-width: 80%; }
  .msg-ai .avatar { width: 32px; height: 32px; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 16px; flex-shrink: 0; margin-top: 2px; }
  .msg-ai .msg-body { flex: 1; }
  .msg-ai .speaker-name { font-size: 11px; font-weight: 600; margin-bottom: 4px; }
  .msg-ai .bubble { background: var(--bg3); padding: 10px 14px; border-radius: 4px 18px 18px 18px; font-size: 14px; line-height: 1.5; white-space: pre-wrap; word-break: break-word; }
  .msg-system { text-align: center; font-size: 11px; color: var(--muted); font-style: italic; padding: 4px; }
  .streaming .bubble { border: 1px solid var(--border); }
  .cursor { display: inline-block; width: 2px; height: 14px; background: var(--indigo); animation: blink 1s infinite; vertical-align: middle; }
  @keyframes blink { 0%,100% { opacity: 1; } 50% { opacity: 0; } }
  #chat-input-area { padding: 12px 16px; background: var(--bg2); border-top: 1px solid var(--border); flex-shrink: 0; }
  #input-row { display: flex; gap: 8px; align-items: flex-end; }
  #msg-input { flex: 1; background: var(--bg3); border: 1px solid var(--border); border-radius: 12px; padding: 10px 14px; color: var(--text); font-size: 14px; resize: none; outline: none; font-family: inherit; line-height: 1.4; max-height: 120px; }
  #msg-input:focus { border-color: var(--indigo); }
  #send-btn { padding: 10px 18px; background: var(--indigo); color: white; border: none; border-radius: 12px; font-size: 14px; font-weight: 600; cursor: pointer; flex-shrink: 0; }
  #send-btn:hover { background: var(--indigo2); }
  #send-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  #stop-btn { padding: 10px 18px; background: var(--red); color: white; border: none; border-radius: 12px; font-size: 14px; font-weight: 600; cursor: pointer; flex-shrink: 0; display: none; }

  /* Settings */
  #settings-page { overflow-y: auto; padding: 32px; }
  .settings-header { margin-bottom: 24px; }
  .settings-header h1 { font-size: 20px; font-weight: 700; }
  .settings-header p { color: var(--muted); font-size: 13px; margin-top: 4px; }
  .provider-card { background: var(--bg2); border: 1px solid var(--border); border-radius: 16px; padding: 20px; margin-bottom: 16px; max-width: 640px; }
  .provider-card.has-key { border-color: #1e4033; }
  .card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
  .card-title { display: flex; align-items: center; gap: 10px; }
  .card-title .icon { font-size: 22px; }
  .card-title h3 { font-size: 14px; font-weight: 600; }
  .card-title small { color: var(--muted); font-size: 11px; }
  .card-badge { font-size: 11px; padding: 3px 10px; border-radius: 20px; }
  .badge-none { background: rgba(100,116,139,0.15); color: var(--muted); }
  .badge-set { background: rgba(16,185,129,0.15); color: #10b981; }
  .card-note { font-size: 12px; color: var(--muted); background: rgba(255,255,255,0.03); padding: 8px 12px; border-radius: 8px; margin-bottom: 12px; line-height: 1.5; }
  .card-inputs { display: flex; gap: 8px; }
  .card-inputs input { flex: 1; background: var(--bg3); border: 1px solid var(--border); border-radius: 10px; padding: 8px 12px; color: var(--text); font-size: 13px; outline: none; font-family: monospace; }
  .card-inputs input:focus { border-color: var(--indigo); }
  .card-btn { padding: 8px 16px; background: var(--indigo); color: white; border: none; border-radius: 10px; font-size: 13px; cursor: pointer; white-space: nowrap; }
  .card-btn:hover { background: var(--indigo2); }
  .card-btn.secondary { background: var(--bg3); border: 1px solid var(--border); color: var(--text); }
  .card-btn.secondary:hover { background: var(--border); }
  .card-actions { display: flex; gap: 8px; margin-top: 8px; align-items: center; }
  .card-test-result { font-size: 12px; margin-top: 6px; }
  .result-ok { color: var(--green); }
  .result-err { color: var(--red); }
  .import-banner { background: rgba(99,102,241,0.1); border: 1px solid rgba(99,102,241,0.3); border-radius: 16px; padding: 16px 20px; margin-bottom: 20px; max-width: 640px; }
  .import-banner h3 { font-size: 13px; font-weight: 600; color: #a5b4fc; margin-bottom: 10px; }
  .import-btns { display: flex; gap: 8px; flex-wrap: wrap; }
  .import-result { font-size: 12px; margin-top: 8px; }
</style>

<div id="layout">
  <nav id="nav">
    <div class="logo">R</div>
    <a class="active" data-page="conf" title="Roundtable">🎙️</a>
    <a data-page="mcp" title="MCP Servers">🔌</a>
    <a data-page="settings" title="Providers">⚙️</a>
  </nav>
  <div id="content">
    <!-- Conference Page -->
    <div class="page active" id="page-conf">
      <div id="conf-inner">
        <div id="rooms-panel">
          <div id="rooms-header">
            <span>🎙️ Rooms</span>
            <button id="add-room-btn">+</button>
          </div>
          <div id="new-room-form">
            <input id="new-room-input" placeholder="Room name..." />
          </div>
          <div id="rooms-list"></div>
          <div id="aichat-status"><div id="status-dot"></div><span id="status-text">ready</span></div>
        </div>
        <div id="chat-area">
          <div id="empty-state">
            <div class="big-icon">🎙️</div>
            <h2>Roundtable AI</h2>
            <p>Create a conference room. ERIN moderates — Claude, Grok, and Gemini debate your question.</p>
            <button id="create-room-big">+ Create Conference Room</button>
          </div>
          <div id="chat-header" style="display:none">
            <span id="chat-title">Room</span>
            <div id="mode-btns">
              <button class="mode-btn active" data-mode="moderator">🤖 ERIN</button>
              <button class="mode-btn" data-mode="parallel">⚡ Para</button>
              <button class="mode-btn" data-mode="roundrobin">🔄 Robin</button>
            </div>
          </div>
          <div id="chat-messages" style="display:none"></div>
          <div id="chat-input-area" style="display:none">
            <div id="input-row">
              <textarea id="msg-input" rows="2" placeholder="Ask the roundtable anything... (Enter to send)"></textarea>
              <button id="stop-btn">⏹ Stop</button>
              <button id="send-btn">Send</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Settings Page -->
    <div class="page" id="page-settings">
      <div id="settings-page">
        <div class="settings-header">
          <h1>⚙️ Provider Settings</h1>
          <p>Configure API keys. Stored in OS keyring — never in plaintext.</p>
        </div>
        <div id="import-banner" class="import-banner" style="display:none">
          <h3>⚡ Desktop apps detected — import sessions</h3>
          <div class="import-btns" id="import-btns"></div>
          <div class="import-result" id="import-result"></div>
        </div>
        <div id="providers-list"></div>
      </div>
    </div>
  <!-- MCP Page -->
    <div class="page" id="page-mcp">
      <div id="settings-page">
        <div class="settings-header">
          <h1>🔌 MCP Connectors</h1>
          <p>Model Context Protocol servers — give ERIN tools like web search, file access, databases.</p>
        </div>
        <div id="mcp-list"></div>
        <div style="margin-top:16px;max-width:640px">
          <button class="card-btn" id="add-mcp-btn" style="margin-bottom:16px">+ Add Server</button>
          <div id="add-mcp-form" style="display:none;background:var(--bg2);border:1px solid var(--border);border-radius:16px;padding:20px;margin-bottom:16px">
            <div style="display:flex;flex-direction:column;gap:10px">
              <input id="mcp-name" placeholder="Name (e.g. Filesystem)" style="background:var(--bg3);border:1px solid var(--border);border-radius:10px;padding:8px 12px;color:var(--text);font-size:13px;outline:none"/>
              <select id="mcp-type" style="background:var(--bg3);border:1px solid var(--border);border-radius:10px;padding:8px 12px;color:var(--text);font-size:13px;outline:none">
                <option value="stdio">stdio (command)</option>
                <option value="http">HTTP/SSE (url)</option>
              </select>
              <input id="mcp-command" placeholder="Command (e.g. npx -y @modelcontextprotocol/server-fetch)" style="background:var(--bg3);border:1px solid var(--border);border-radius:10px;padding:8px 12px;color:var(--text);font-size:13px;outline:none;font-family:monospace"/>
              <div style="display:flex;gap:8px">
                <button class="card-btn" id="mcp-save-btn">Add</button>
                <button class="card-btn secondary" id="mcp-cancel-btn">Cancel</button>
              </div>
            </div>
          </div>
        </div>
        <div style="border:1px solid var(--border);border-radius:12px;padding:16px;max-width:640px;font-size:12px;color:var(--muted);line-height:1.7">
          <strong style="color:var(--text)">Quick add presets:</strong><br/>
          <div style="display:flex;flex-wrap:wrap;gap:8px;margin-top:10px" id="mcp-presets"></div>
        </div>
      </div>
    </div>
  </div>
</div>
`

// ── Navigation ──
document.querySelectorAll('#nav a').forEach(a => {
  a.addEventListener('click', e => {
    e.preventDefault()
    const page = a.dataset.page
    document.querySelectorAll('#nav a').forEach(x => x.classList.remove('active'))
    document.querySelectorAll('.page').forEach(x => x.classList.remove('active'))
    a.classList.add('active')
    document.getElementById(`page-${page}`).classList.add('active')
    if (page === 'settings') loadSettings()
    if (page === 'mcp') loadMCP()
  })
})

// ── Model colors ──
const MODEL_INFO = {
  'claude-opus-4-5': { label: 'Claude', color: '#d97706', emoji: '🟠' },
  'claude-sonnet-4-5': { label: 'Claude', color: '#d97706', emoji: '🟠' },
  'claude-web': { label: 'Claude', color: '#d97706', emoji: '🟠' },
  'grok-3': { label: 'Grok', color: '#6366f1', emoji: '🟣' },
  'grok-web': { label: 'Grok', color: '#6366f1', emoji: '🟣' },
  'gemini-2.5-pro': { label: 'Gemini', color: '#10b981', emoji: '🟢' },
  'erin': { label: 'ERIN', color: '#ef4444', emoji: '🔴' },
  'user': { label: 'You', color: '#6366f1', emoji: '👤' },
  'System': { label: 'System', color: '#64748b', emoji: '⚙️' },
}
function modelInfo(speaker) {
  if (!speaker) return { label: 'AI', color: '#64748b', emoji: '🤖' }
  if (MODEL_INFO[speaker]) return MODEL_INFO[speaker]
  const k = Object.keys(MODEL_INFO).find(k => speaker.includes(k) || k.includes(speaker))
  return k ? MODEL_INFO[k] : { label: speaker, color: '#64748b', emoji: '🤖' }
}

// ── Rooms ──
async function loadRooms() {
  try {
    rooms = await invoke('list_rooms')
    renderRooms()
  } catch(e) { console.error('loadRooms:', e) }
}

function renderRooms() {
  const list = document.getElementById('rooms-list')
  list.innerHTML = ''
  if (rooms.length === 0) {
    list.innerHTML = '<div style="padding:16px;text-align:center;font-size:12px;color:var(--muted)">No rooms yet.<br/>Click + to create one.</div>'
    return
  }
  rooms.forEach(r => {
    const div = document.createElement('div')
    div.className = 'room-item' + (activeRoom?.id === r.id ? ' active' : '')
    div.innerHTML = `<span>${r.mode === 'moderator' ? '🤖' : r.mode === 'parallel' ? '⚡' : '🔄'} ${r.id}</span><span class="room-del" data-id="${r.id}">✕</span>`
    div.addEventListener('click', () => switchRoom(r))
    div.querySelector('.room-del').addEventListener('click', async e => {
      e.stopPropagation()
      await invoke('delete_room', { roomId: r.id })
      if (activeRoom?.id === r.id) { activeRoom = null; showEmpty() }
      loadRooms()
    })
    list.appendChild(div)
  })
}

async function switchRoom(r) {
  activeRoom = r
  document.querySelectorAll('.room-item').forEach(x => x.classList.remove('active'))
  document.querySelectorAll('.room-item').forEach(x => {
    if (x.textContent.includes(r.id)) x.classList.add('active')
  })
  showChat()
  document.getElementById('chat-title').textContent = r.id
  // Set mode button
  document.querySelectorAll('.mode-btn').forEach(b => {
    b.classList.toggle('active', b.dataset.mode === r.mode)
  })
  // Load messages
  try {
    messages = await invoke('get_room_messages', { roomId: r.id })
    renderMessages()
  } catch(e) { messages = []; renderMessages() }
}

function showEmpty() {
  document.getElementById('empty-state').style.display = 'flex'
  document.getElementById('chat-header').style.display = 'none'
  document.getElementById('chat-messages').style.display = 'none'
  document.getElementById('chat-input-area').style.display = 'none'
}

function showChat() {
  document.getElementById('empty-state').style.display = 'none'
  document.getElementById('chat-header').style.display = 'flex'
  document.getElementById('chat-messages').style.display = 'flex'
  document.getElementById('chat-input-area').style.display = 'block'
}

// ── Add room ──
const addBtn = document.getElementById('add-room-btn')
const newForm = document.getElementById('new-room-form')
const newInput = document.getElementById('new-room-input')

addBtn.addEventListener('click', () => {
  newForm.style.display = newForm.style.display === 'none' ? 'block' : 'none'
  if (newForm.style.display === 'block') newInput.focus()
})

document.getElementById('create-room-big').addEventListener('click', () => {
  newForm.style.display = 'block'
  newInput.value = 'conference-1'
  newInput.focus()
})

newInput.addEventListener('keydown', async e => {
  if (e.key === 'Enter') {
    const name = newInput.value.trim()
    if (!name) return
    newInput.value = ''
    newForm.style.display = 'none'
    try {
      await invoke('create_room', {
        roomId: name,
        participants: ['claude-opus-4-5', 'grok-3', 'gemini-2.5-pro', 'erin'],
        moderatorModel: 'erin',
        mode: 'moderator'
      })
      await loadRooms()
      const r = rooms.find(x => x.id === name)
      if (r) switchRoom(r)
    } catch(e) { alert('Error: ' + e) }
  }
  if (e.key === 'Escape') { newForm.style.display = 'none' }
})

// ── Mode switch ──
document.querySelectorAll('.mode-btn').forEach(b => {
  b.addEventListener('click', async () => {
    if (!activeRoom) return
    const mode = b.dataset.mode
    document.querySelectorAll('.mode-btn').forEach(x => x.classList.remove('active'))
    b.classList.add('active')
    activeRoom.mode = mode
    await invoke('set_room_mode', { roomId: activeRoom.id, mode })
  })
})

// ── Messages ──
function renderMessages() {
  const el = document.getElementById('chat-messages')
  el.innerHTML = ''
  messages.forEach(m => el.appendChild(renderMsg(m)))
  // Streaming buffers
  Object.entries(streamBuffers).forEach(([speaker, text]) => {
    if (text) el.appendChild(renderStreaming(speaker, text))
  })
  if (isGenerating && Object.keys(streamBuffers).length === 0) {
    const thinking = document.createElement('div')
    thinking.className = 'msg-system'
    thinking.textContent = 'ERIN moderating...'
    el.appendChild(thinking)
  }
  el.scrollTop = el.scrollHeight
}

function renderMsg(m) {
  if (m.speaker === 'user' || m.role === 'user') {
    const div = document.createElement('div')
    div.className = 'msg-user'
    div.innerHTML = `<div class="bubble">${escHtml(m.content)}</div>`
    return div
  }
  if (!m.speaker || m.speaker === 'System' || m.speaker?.includes('[System]') || m.speaker?.includes('[Moderator]')) {
    const div = document.createElement('div')
    div.className = 'msg-system'
    div.textContent = m.content.replace(/^📋 \*|\*$/g, '')
    return div
  }
  const info = modelInfo(m.speaker)
  const div = document.createElement('div')
  div.className = 'msg-ai'
  div.innerHTML = `
    <div class="avatar" style="background:${info.color}22;border:1px solid ${info.color}44">${info.emoji}</div>
    <div class="msg-body">
      <div class="speaker-name" style="color:${info.color}">${m.speaker}</div>
      <div class="bubble">${escHtml(m.content)}</div>
    </div>`
  return div
}

function renderStreaming(speaker, text) {
  const info = modelInfo(speaker)
  const div = document.createElement('div')
  div.className = 'msg-ai streaming'
  div.innerHTML = `
    <div class="avatar" style="background:${info.color}22;border:1px solid ${info.color}44">${info.emoji}</div>
    <div class="msg-body">
      <div class="speaker-name" style="color:${info.color}">${speaker}</div>
      <div class="bubble">${escHtml(text)}<span class="cursor"></span></div>
    </div>`
  return div
}

function escHtml(s) {
  return (s || '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/\n/g,'<br>')
}

// ── Send ──
const msgInput = document.getElementById('msg-input')
const sendBtn = document.getElementById('send-btn')
const stopBtn = document.getElementById('stop-btn')

msgInput.addEventListener('keydown', e => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send() }
})
sendBtn.addEventListener('click', send)
stopBtn.addEventListener('click', async () => {
  if (activeRoom) await invoke('cancel_generation', { roomId: activeRoom.id }).catch(()=>{})
  setGenerating(false)
})

async function send() {
  const text = msgInput.value.trim()
  if (!text || isGenerating || !activeRoom) return
  msgInput.value = ''
  setGenerating(true)
  messages.push({ role: 'user', content: text, speaker: 'user', ts: Date.now() })
  renderMessages()
  try {
    await invoke('send_message', { roomId: activeRoom.id, content: text })
  } catch(e) {
    messages.push({ role: 'assistant', content: '⚠️ Error: ' + e, speaker: 'System' })
    setGenerating(false)
    renderMessages()
  }
}

function setGenerating(v) {
  isGenerating = v
  sendBtn.style.display = v ? 'none' : 'block'
  stopBtn.style.display = v ? 'block' : 'none'
  msgInput.disabled = v
}

// ── Events from Rust ──
async function initEvents() {
  await listen('stream-delta', e => {
    const { room_id, speaker, delta } = e.payload
    if (room_id !== activeRoom?.id) return
    streamBuffers[speaker] = (streamBuffers[speaker] || '') + delta
    renderMessages()
  })
  await listen('stream-end', e => {
    const { room_id, speaker } = e.payload
    if (room_id !== activeRoom?.id) return
    delete streamBuffers[speaker]
    renderMessages()
  })
  await listen('turn-complete', async e => {
    const { room_id } = e.payload
    streamBuffers = {}
    setGenerating(false)
    if (room_id === activeRoom?.id) {
      messages = await invoke('get_room_messages', { roomId: room_id }).catch(() => messages)
      renderMessages()
    }
    loadRooms()
  })
  await listen('turn-error', e => {
    const { room_id, error } = e.payload
    streamBuffers = {}
    setGenerating(false)
    if (room_id === activeRoom?.id) {
      messages.push({ role: 'assistant', content: '⚠️ ' + error, speaker: 'System' })
      renderMessages()
    }
  })
}

// ── Settings ──
const PROVIDERS = [
  { id: 'erin', label: 'ERIN (Local AGI)', icon: '🔴', note: 'Your local AGI on Transformers at 10.1.1.19:5000. No key needed.', noKey: true },
  { id: 'anthropic', label: 'Claude (API Key)', icon: '🟠', note: 'API key from console.anthropic.com', placeholder: 'sk-ant-api03-...' },
  { id: 'anthropic-web', label: 'Claude (Desktop Session)', icon: '🟠', note: 'Import session from Claude Desktop app', session: true },
  { id: 'xai', label: 'Grok (API Key)', icon: '🟣', note: 'API key from console.x.ai', placeholder: 'xai-...' },
  { id: 'xai-web', label: 'Grok (Desktop Session)', icon: '🟣', note: 'Import session from Grok Desktop app', session: true },
  { id: 'google', label: 'Gemini', icon: '🟢', note: 'Free API key from aistudio.google.com', placeholder: 'AIzaSy...' },
  { id: 'openai', label: 'OpenAI', icon: '⚫', note: 'API key from platform.openai.com', placeholder: 'sk-proj-...' },
  { id: 'github-copilot', label: 'GitHub Copilot', icon: '⚪', note: 'PAT with copilot scope from github.com/settings/tokens', placeholder: 'github_pat_...' },
]

let providerStatuses = {}

async function loadSettings() {
  try {
    const list = await invoke('get_auth_status')
    providerStatuses = {}
    list.forEach(s => providerStatuses[s.provider_id] = s)
  } catch(e) {}

  // Check for installed apps
  try {
    const apps = await invoke('check_installed_apps')
    const banner = document.getElementById('import-banner')
    const btns = document.getElementById('import-btns')
    btns.innerHTML = ''
    if (apps.claude_desktop || apps.grok_desktop) {
      banner.style.display = 'block'
      if (apps.claude_desktop) {
        const b = document.createElement('button')
        b.className = 'card-btn'
        b.style.background = '#92400e'
        b.textContent = '🟠 Import Claude Session'
        b.onclick = async () => {
          const r = await invoke('import_claude_session')
          document.getElementById('import-result').textContent = r.message
          document.getElementById('import-result').style.color = r.success ? '#10b981' : '#ef4444'
          loadSettings()
        }
        btns.appendChild(b)
      }
      if (apps.grok_desktop) {
        const b = document.createElement('button')
        b.className = 'card-btn'
        b.style.background = '#3730a3'
        b.textContent = '🟣 Import Grok Session'
        b.onclick = async () => {
          const r = await invoke('import_grok_session')
          document.getElementById('import-result').textContent = r.message
          document.getElementById('import-result').style.color = r.success ? '#10b981' : '#ef4444'
          loadSettings()
        }
        btns.appendChild(b)
      }
    }
  } catch(e) {}

  renderProviders()
}

function renderProviders() {
  const list = document.getElementById('providers-list')
  list.innerHTML = ''
  PROVIDERS.forEach(p => {
    const s = providerStatuses[p.id]
    const hasKey = s?.has_key
    const card = document.createElement('div')
    card.className = 'provider-card' + (hasKey ? ' has-key' : '')

    let inputsHtml = ''
    if (!p.noKey && !p.session) {
      inputsHtml = `
        <div class="card-inputs">
          <input type="password" id="input-${p.id}" placeholder="${hasKey ? '••••• (set — paste new to update)' : (p.placeholder || '')}"/>
          <button class="card-btn" id="save-${p.id}">Save</button>
        </div>
        <div class="card-actions">
          ${hasKey ? `<button class="card-btn secondary" id="test-${p.id}">🔌 Test</button><button class="card-btn secondary" id="del-${p.id}" style="color:#ef4444">🗑</button>` : ''}
        </div>
        <div class="card-test-result" id="result-${p.id}"></div>`
    } else if (p.noKey) {
      inputsHtml = `
        <div class="card-actions">
          <button class="card-btn secondary" id="test-${p.id}">🔌 Test Connection</button>
        </div>
        <div class="card-test-result" id="result-${p.id}"></div>`
    } else {
      // session
      inputsHtml = hasKey
        ? `<div style="font-size:12px;color:var(--green)">✅ Session active</div>
           <button class="card-btn secondary" id="del-${p.id}" style="margin-top:8px;color:#ef4444;font-size:12px">Clear session</button>`
        : `<div style="font-size:12px;color:var(--muted)">Use Import button above</div>`
    }

    card.innerHTML = `
      <div class="card-header">
        <div class="card-title">
          <span class="icon">${p.icon}</span>
          <div><h3>${p.label}</h3></div>
        </div>
        <span class="card-badge ${hasKey ? 'badge-set' : 'badge-none'}">${hasKey ? '✓ Set' : 'Not set'}</span>
      </div>
      <div class="card-note">${p.note}</div>
      ${inputsHtml}`
    list.appendChild(card)

    // Attach events
    const saveBtn = document.getElementById(`save-${p.id}`)
    if (saveBtn) {
      saveBtn.addEventListener('click', async () => {
        const key = document.getElementById(`input-${p.id}`).value.trim()
        if (!key) return
        try {
          await invoke('save_api_key', { providerId: p.id, apiKey: key })
          document.getElementById(`input-${p.id}`).value = ''
          loadSettings()
        } catch(e) { alert('Error: ' + e) }
      })
    }

    const testBtn = document.getElementById(`test-${p.id}`)
    if (testBtn) {
      testBtn.addEventListener('click', async () => {
        const el = document.getElementById(`result-${p.id}`)
        el.textContent = 'Testing...'
        el.className = 'card-test-result'
        try {
          const msg = await invoke('test_connection', { providerId: p.id })
          el.textContent = msg
          el.className = 'card-test-result result-ok'
        } catch(e) {
          el.textContent = String(e)
          el.className = 'card-test-result result-err'
        }
      })
    }

    const delBtn = document.getElementById(`del-${p.id}`)
    if (delBtn) {
      delBtn.addEventListener('click', async () => {
        await invoke('delete_api_key', { providerId: p.id })
        loadSettings()
      })
    }
  })
}

// ── MCP ──
const MCP_PRESETS = [
  { name: 'Fetch/Web', command: 'npx -y @modelcontextprotocol/server-fetch' },
  { name: 'Filesystem', command: 'npx -y @modelcontextprotocol/server-filesystem /' },
  { name: 'Git', command: 'npx -y @modelcontextprotocol/server-git' },
  { name: 'Postgres', command: 'npx -y @modelcontextprotocol/server-postgres postgresql://localhost/mydb' },
  { name: 'ERIN Memory', command: 'http://10.1.1.19:5456', type: 'http' },
]

async function loadMCP() {
  let servers = []
  try {
    const raw = await invoke('get_config_value', { key: 'mcp-servers' })
    if (raw) servers = JSON.parse(raw)
  } catch(e) {}

  // Always show local-system as first entry
  const list = document.getElementById('mcp-list')
  list.innerHTML = ''
  const builtIn = {
    id: 'local-system', name: 'Local System (built-in)', type: 'stdio',
    command: 'node /opt/mcp-local-server/dist/index.js',
    enabled: true, builtin: true,
    desc: 'File system, processes, Docker, cron — already connected'
  }
  ;[builtIn, ...servers].forEach(s => {
    const div = document.createElement('div')
    div.className = 'provider-card' + (s.enabled ? ' has-key' : '')
    div.innerHTML = `
      <div class="card-header">
        <div class="card-title">
          <span class="icon">${s.type === 'http' ? '🌐' : '⚡'}</span>
          <div><h3>${s.name}</h3><small>${s.desc || s.command || ''}</small></div>
        </div>
        <div style="display:flex;gap:8px;align-items:center">
          <span class="card-badge ${s.enabled ? 'badge-set' : 'badge-none'}">${s.type.toUpperCase()}</span>
          ${!s.builtin ? `<button onclick="deleteMCP('${s.id}')" style="background:none;border:none;color:var(--muted);cursor:pointer;font-size:16px">✕</button>` : ''}
        </div>
      </div>`
    list.appendChild(div)
  })

  // Presets
  const presets = document.getElementById('mcp-presets')
  if (presets && presets.children.length === 0) {
    MCP_PRESETS.forEach(p => {
      const b = document.createElement('button')
      b.className = 'card-btn secondary'
      b.style.fontSize = '12px'
      b.textContent = '+ ' + p.name
      b.onclick = () => addMCPPreset(p, servers)
      presets.appendChild(b)
    })
  }
}

window.deleteMCP = async function(id) {
  const raw = await invoke('get_config_value', { key: 'mcp-servers' }).catch(() => null)
  const servers = raw ? JSON.parse(raw).filter(s => s.id !== id) : []
  await invoke('save_config_value', { key: 'mcp-servers', value: JSON.stringify(servers) })
  loadMCP()
}

async function addMCPPreset(p, existing) {
  const servers = [...existing, {
    id: 'mcp-' + Date.now(), name: p.name,
    type: p.type || 'stdio', command: p.command, enabled: true
  }]
  await invoke('save_config_value', { key: 'mcp-servers', value: JSON.stringify(servers) })
  loadMCP()
}

document.getElementById('add-mcp-btn').addEventListener('click', () => {
  document.getElementById('add-mcp-form').style.display = 'block'
})
document.getElementById('mcp-cancel-btn').addEventListener('click', () => {
  document.getElementById('add-mcp-form').style.display = 'none'
})
document.getElementById('mcp-save-btn').addEventListener('click', async () => {
  const name = document.getElementById('mcp-name').value.trim()
  const type = document.getElementById('mcp-type').value
  const command = document.getElementById('mcp-command').value.trim()
  if (!name || !command) return
  const raw = await invoke('get_config_value', { key: 'mcp-servers' }).catch(() => null)
  const servers = raw ? JSON.parse(raw) : []
  servers.push({ id: 'mcp-' + Date.now(), name, type, command, enabled: true })
  await invoke('save_config_value', { key: 'mcp-servers', value: JSON.stringify(servers) })
  document.getElementById('add-mcp-form').style.display = 'none'
  document.getElementById('mcp-name').value = ''
  document.getElementById('mcp-command').value = ''
  loadMCP()
})

// ── Init ──
async function init() {
  await initEvents()
  await loadRooms()
  if (rooms.length > 0) switchRoom(rooms[0])
}

init()
