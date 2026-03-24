import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

let rooms=[], activeRoom=null, messages=[], streamBuffers={}, isGenerating=false
let providerStatuses={}, selectedModels={}, configValues={}

const PROVIDERS = [
  { id:'erin', label:'ERIN', subtitle:'Local AGI — Transformers', icon:'🔴', noKey:true,
    note:'Your local AGI on Transformers at 10.1.1.19:5000. No API key needed.',
    models:[{id:'erin',label:'ERIN (default)'},{id:'erin-v1-12b-q4km',label:'erin-v1-12b-q4km'}],
    configKeys:[{key:'erin-endpoint',label:'Endpoint',placeholder:'http://10.1.1.19:5000',def:'http://10.1.1.19:5000'}] },
  { id:'anthropic', label:'Claude', subtitle:'Anthropic API', icon:'🟠',
    placeholder:'sk-ant-api03-...', note:'API key from console.anthropic.com',
    consoleUrl:'https://console.anthropic.com/settings/keys',
    models:[{id:'claude-opus-4-5',label:'Claude Opus 4.5'},{id:'claude-sonnet-4-5',label:'Claude Sonnet 4.5'},{id:'claude-haiku-4-5',label:'Claude Haiku 4.5 (cheap)'}] },
  { id:'anthropic-web', label:'Claude', subtitle:'Desktop Session', icon:'🟠', session:true,
    note:'Uses your Claude Desktop session — no API cost.',
    models:[{id:'claude-web',label:'Claude (web session)'}] },
  { id:'xai', label:'Grok', subtitle:'xAI API', icon:'🟣',
    placeholder:'xai-...', note:'API key from console.x.ai',
    consoleUrl:'https://console.x.ai/',
    models:[{id:'grok-3',label:'Grok 3'},{id:'grok-3-mini',label:'Grok 3 Mini (fast)'}] },
  { id:'xai-web', label:'Grok', subtitle:'Desktop Session', icon:'🟣', session:true,
    note:'Uses your Grok Desktop session.',
    models:[{id:'grok-web',label:'Grok (web session)'}] },
  { id:'google', label:'Gemini', subtitle:'Google AI Studio', icon:'🟢',
    placeholder:'AIzaSy...', note:'Free API key from aistudio.google.com',
    consoleUrl:'https://aistudio.google.com/app/apikey',
    models:[{id:'gemini-2.5-pro',label:'Gemini 2.5 Pro'},{id:'gemini-2.0-flash',label:'Gemini 2.0 Flash (fast)'},{id:'gemini-1.5-pro',label:'Gemini 1.5 Pro'}] },
  { id:'openai', label:'OpenAI', subtitle:'GPT / o-series', icon:'⚫',
    placeholder:'sk-proj-...', note:'API key from platform.openai.com',
    consoleUrl:'https://platform.openai.com/api-keys',
    models:[{id:'gpt-4o',label:'GPT-4o'},{id:'gpt-4o-mini',label:'GPT-4o Mini (cheap)'},{id:'o3-mini',label:'o3-mini (reasoning)'}] },
  { id:'github-copilot', label:'GitHub Copilot', subtitle:'Subscription models', icon:'⚪',
    placeholder:'github_pat_...', note:'PAT with copilot scope — access GPT-4o, Claude, o3 via subscription.',
    consoleUrl:'https://github.com/settings/tokens/new?scopes=copilot',
    models:[{id:'github/gpt-4o',label:'GPT-4o via Copilot'},{id:'github/claude-3-5-sonnet',label:'Claude 3.5 via Copilot'},{id:'github/o3-mini',label:'o3-mini via Copilot'}] },
  { id:'mistral', label:'Mistral AI', subtitle:'La Plateforme', icon:'🟡',
    placeholder:'mistral-...', note:'API key from console.mistral.ai',
    consoleUrl:'https://console.mistral.ai/api-keys',
    models:[{id:'mistral-large-latest',label:'Mistral Large'},{id:'mistral-small-latest',label:'Mistral Small (cheap)'}] },
  { id:'ollama', label:'Ollama', subtitle:'Local — configurable', icon:'🔵', noKey:true,
    note:'Local Ollama. Set endpoint and model below.',
    models:[],
    configKeys:[{key:'ollama-endpoint',label:'Endpoint',placeholder:'http://localhost:11434',def:'http://localhost:11434'},{key:'ollama-model',label:'Model',placeholder:'llama3.2',def:'llama3.2'}] },
]

const CSS = `
:root{--bg:#030712;--bg2:#0f172a;--bg3:#1e293b;--bdr:#334155;--text:#f1f5f9;--muted:#64748b;--ind:#6366f1;--ind2:#4f46e5;--red:#ef4444;--grn:#10b981}
*{box-sizing:border-box;margin:0;padding:0}
body{background:var(--bg);color:var(--text);font-family:system-ui,sans-serif;height:100vh;overflow:hidden}
#L{display:flex;height:100vh;overflow:hidden}
#nav{width:56px;background:var(--bg2);border-right:1px solid var(--bdr);display:flex;flex-direction:column;align-items:center;padding:12px 0;gap:8px;flex-shrink:0}
.nl{width:36px;height:36px;background:var(--ind);border-radius:10px;display:flex;align-items:center;justify-content:center;font-weight:900;font-size:14px;color:#fff;margin-bottom:8px}
.nb{width:40px;height:40px;border-radius:10px;display:flex;align-items:center;justify-content:center;font-size:20px;cursor:pointer;border:none;background:none;color:var(--text);transition:background .15s}
.nb:hover{background:var(--bg3)}.nb.act{background:var(--ind)}
#C{flex:1;overflow:hidden;display:flex;flex-direction:column}
.pg{display:none;flex:1;flex-direction:column;overflow:hidden}.pg.act{display:flex}
#cw{display:flex;height:100%;overflow:hidden}
#rp{width:200px;background:var(--bg2);border-right:1px solid var(--bdr);display:flex;flex-direction:column;flex-shrink:0}
#rh{padding:12px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--bdr)}
#rh span{font-weight:600;font-size:13px}
#arb{width:26px;height:26px;background:var(--ind);border-radius:50%;border:none;color:#fff;font-size:20px;cursor:pointer;display:flex;align-items:center;justify-content:center}
#nrf{padding:8px;display:none;border-bottom:1px solid var(--bdr)}
#nri{width:100%;background:var(--bg3);border:1px solid var(--bdr);border-radius:8px;padding:6px 8px;color:var(--text);font-size:12px;outline:none}
#nri:focus{border-color:var(--ind)}
#rl{flex:1;overflow-y:auto;padding:4px}
.ri{padding:8px 10px;border-radius:8px;cursor:pointer;font-size:12px;color:var(--muted);margin:2px 0;display:flex;align-items:center;justify-content:space-between}
.ri:hover,.ri.act{background:var(--bg3);color:var(--text)}
.rd{opacity:0;font-size:10px;cursor:pointer;padding:2px 4px}.ri:hover .rd{opacity:1}
#rf{padding:8px 12px;font-size:11px;color:var(--muted);border-top:1px solid var(--bdr);display:flex;align-items:center;gap:6px}
#sd{width:6px;height:6px;border-radius:50%;background:#f59e0b;flex-shrink:0}
#ca{flex:1;display:flex;flex-direction:column;overflow:hidden}
#es{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;color:var(--muted);text-align:center;padding:40px}
#es .ei{font-size:64px}#es h2{color:var(--text);font-size:22px}#es p{font-size:14px;line-height:1.6;max-width:360px}
#cb{padding:12px 24px;background:var(--ind);color:#fff;border:none;border-radius:12px;font-size:15px;font-weight:600;cursor:pointer}
#cb:hover{background:var(--ind2)}
#ch{padding:10px 16px;background:var(--bg2);border-bottom:1px solid var(--bdr);display:none;align-items:center;justify-content:space-between;flex-shrink:0}
#ct{font-weight:600;font-size:14px}
#mb{display:flex;gap:4px;background:var(--bg3);border-radius:8px;padding:3px}
.mb{padding:4px 10px;border-radius:6px;border:none;background:none;color:var(--muted);font-size:11px;cursor:pointer}
.mb.act{background:var(--ind);color:#fff}
#ms{flex:1;overflow-y:auto;padding:16px;display:none;flex-direction:column;gap:12px}
.mu{display:flex;justify-content:flex-end}
.mu .b{background:var(--ind);color:#fff;padding:10px 14px;border-radius:18px 18px 4px 18px;max-width:70%;font-size:14px;line-height:1.5;word-break:break-word}
.ma{display:flex;gap:10px;max-width:85%}
.av{width:32px;height:32px;border-radius:50%;display:flex;align-items:center;justify-content:center;font-size:16px;flex-shrink:0;margin-top:2px}
.ab{flex:1}.an{font-size:11px;font-weight:600;margin-bottom:4px}
.at{background:var(--bg3);padding:10px 14px;border-radius:4px 18px 18px 18px;font-size:14px;line-height:1.5;white-space:pre-wrap;word-break:break-word}
.sy{text-align:center;font-size:11px;color:var(--muted);font-style:italic;padding:4px}
.cur{display:inline-block;width:2px;height:14px;background:var(--ind);animation:blink 1s infinite;vertical-align:middle;margin-left:2px}
@keyframes blink{0%,100%{opacity:1}50%{opacity:0}}
#ia{padding:12px 16px;background:var(--bg2);border-top:1px solid var(--bdr);flex-shrink:0;display:none}
#ir{display:flex;gap:8px;align-items:flex-end}
#mi{flex:1;background:var(--bg3);border:1px solid var(--bdr);border-radius:12px;padding:10px 14px;color:var(--text);font-size:14px;resize:none;outline:none;font-family:inherit;line-height:1.4;max-height:120px}
#mi:focus{border-color:var(--ind)}
#sb,#xb{padding:10px 18px;color:#fff;border:none;border-radius:12px;font-size:14px;font-weight:600;cursor:pointer;flex-shrink:0}
#sb{background:var(--ind)}#sb:hover{background:var(--ind2)}#sb:disabled{opacity:.4;cursor:not-allowed}
#xb{background:var(--red);display:none}
.sp{overflow-y:auto;padding:28px 32px;flex:1}
.pt{font-size:20px;font-weight:700;margin-bottom:4px}.ps{color:var(--muted);font-size:13px;margin-bottom:24px}
.ib{background:rgba(99,102,241,.1);border:1px solid rgba(99,102,241,.3);border-radius:16px;padding:16px 20px;margin-bottom:20px;max-width:700px}
.ib h3{font-size:13px;font-weight:600;color:#a5b4fc;margin-bottom:10px}
.ibs{display:flex;gap:8px;flex-wrap:wrap}.ir2{font-size:12px;margin-top:8px}
.pc{background:var(--bg2);border:1px solid var(--bdr);border-radius:16px;padding:20px;margin-bottom:14px;max-width:700px;transition:border-color .2s}
.pc.ok{border-color:#1e4033}.pc.dim{opacity:.5}
.ph{display:flex;align-items:center;justify-content:space-between;margin-bottom:8px}
.pt2{display:flex;align-items:center;gap:10px}
.pi{font-size:22px}.pn{font-size:14px;font-weight:600}.psu{font-size:11px;color:var(--muted)}
.pbg{font-size:11px;padding:3px 10px;border-radius:20px;border:1px solid transparent}
.bn{background:rgba(100,116,139,.15);color:var(--muted);border-color:rgba(100,116,139,.2)}
.bk{background:rgba(16,185,129,.15);color:var(--grn);border-color:rgba(16,185,129,.2)}
.bl{background:rgba(99,102,241,.15);color:#a5b4fc;border-color:rgba(99,102,241,.2)}
.pno{font-size:12px;color:var(--muted);background:rgba(255,255,255,.03);padding:8px 12px;border-radius:8px;margin-bottom:10px;line-height:1.5}
.ps2{margin-top:10px}.pl{font-size:11px;color:var(--muted);display:block;margin-bottom:4px}
.pr{display:flex;gap:8px;align-items:center}
.pi2{flex:1;background:var(--bg3);border:1px solid var(--bdr);border-radius:10px;padding:7px 10px;color:var(--text);font-size:13px;outline:none}
.pi2:focus{border-color:var(--ind)}.pi2:disabled{color:var(--muted)}
.pse{width:100%;background:var(--bg3);border:1px solid var(--bdr);border-radius:10px;padding:7px 10px;color:var(--text);font-size:13px;outline:none}
.pse:disabled{color:var(--muted)}
.btn{padding:7px 14px;border:none;border-radius:10px;font-size:13px;cursor:pointer;font-weight:500;white-space:nowrap}
.bp{background:var(--ind);color:#fff}.bp:hover{background:var(--ind2)}
.bs{background:var(--bg3);border:1px solid var(--bdr);color:var(--text)}.bs:hover{background:var(--bdr)}
.bd{background:none;border:none;color:var(--red);font-size:12px;cursor:pointer;padding:4px 8px}
.bsm{padding:5px 10px;font-size:12px}
.tr{font-size:12px;margin-top:6px}.tok{color:var(--grn)}.ter{color:var(--red)}
`

document.getElementById('app').innerHTML = `
<style>${CSS}</style>
<div id="L">
  <nav id="nav">
    <div class="nl">R</div>
    <button class="nb act" data-pg="conf" title="Roundtable">🎙️</button>
    <button class="nb" data-pg="mcp" title="MCP">🔌</button>
    <button class="nb" data-pg="settings" title="Providers">⚙️</button>
  </nav>
  <div id="C">
    <div class="pg act" id="pg-conf">
      <div id="cw">
        <div id="rp">
          <div id="rh"><span>🎙️ Rooms</span><button id="arb">+</button></div>
          <div id="nrf"><input id="nri" placeholder="Room name…"/></div>
          <div id="rl"></div>
          <div id="rf"><div id="sd"></div><span id="st">ready</span></div>
        </div>
        <div id="ca">
          <div id="es">
            <div class="ei">🎙️</div>
            <h2>Roundtable AI</h2>
            <p>Create a room. ERIN moderates — Claude, Grok, Gemini debate your question.</p>
            <button id="cb">+ Create Conference Room</button>
          </div>
          <div id="ch"><span id="ct"></span>
            <div id="mb">
              <button class="mb act" data-mode="moderator">🤖 ERIN</button>
              <button class="mb" data-mode="parallel">⚡ Para</button>
              <button class="mb" data-mode="roundrobin">🔄 Robin</button>
            </div>
          </div>
          <div id="ms"></div>
          <div id="ia"><div id="ir">
            <textarea id="mi" rows="2" placeholder="Ask the roundtable… (Enter to send)"></textarea>
            <button id="xb">⏹ Stop</button>
            <button id="sb">Send</button>
          </div></div>
        </div>
      </div>
    </div>
    <div class="pg" id="pg-mcp">
      <div class="sp">
        <div class="pt">🔌 MCP Connectors</div>
        <div class="ps">Give ERIN and other AIs tools — file system, web search, databases, Docker.</div>
        <div id="mcpl"></div>
        <button class="btn bp" id="amb" style="margin-bottom:12px">+ Add Server</button>
        <div id="amf" style="display:none" class="pc">
          <div style="display:flex;flex-direction:column;gap:10px">
            <input class="pi2" id="mn" placeholder="Server name"/>
            <select class="pse" id="mt"><option value="stdio">stdio (command)</option><option value="http">HTTP/SSE (url)</option></select>
            <input class="pi2" id="mc" placeholder="npx -y @modelcontextprotocol/server-fetch" style="font-family:monospace"/>
            <div style="display:flex;gap:8px">
              <button class="btn bp bsm" id="ams">Add</button>
              <button class="btn bs bsm" id="amc">Cancel</button>
            </div>
          </div>
        </div>
        <div class="pc" style="max-width:700px">
          <div style="font-size:12px;color:var(--muted);font-weight:600;margin-bottom:8px">Quick-add presets:</div>
          <div style="display:flex;flex-wrap:wrap;gap:8px" id="mpp"></div>
        </div>
      </div>
    </div>
    <div class="pg" id="pg-settings">
      <div class="sp">
        <div class="pt">⚙️ Provider Settings</div>
        <div class="ps">Configure API keys. Stored in OS keyring — never in plaintext.</div>
        <div class="ib" id="impb" style="display:none">
          <h3>⚡ Desktop apps detected — import sessions</h3>
          <div class="ibs" id="imps"></div>
          <div class="ir2" id="impr"></div>
        </div>
        <div id="pvl"></div>
      </div>
    </div>
  </div>
</div>`

// NAV
document.querySelectorAll('.nb').forEach(b => {
  b.addEventListener('click', () => {
    document.querySelectorAll('.nb').forEach(x=>x.classList.remove('act'))
    document.querySelectorAll('.pg').forEach(x=>x.classList.remove('act'))
    b.classList.add('act')
    document.getElementById('pg-'+b.dataset.pg).classList.add('act')
    if(b.dataset.pg==='settings') loadSettings()
    if(b.dataset.pg==='mcp') loadMCP()
  })
})

// MODEL INFO
const MI={
  'claude-opus-4-5':{l:'Claude',c:'#d97706',e:'🟠'},'claude-sonnet-4-5':{l:'Claude',c:'#d97706',e:'🟠'},
  'claude-web':{l:'Claude',c:'#d97706',e:'🟠'},'grok-3':{l:'Grok',c:'#6366f1',e:'🟣'},
  'grok-web':{l:'Grok',c:'#6366f1',e:'🟣'},'gemini-2.5-pro':{l:'Gemini',c:'#10b981',e:'🟢'},
  'gemini-2.0-flash':{l:'Gemini',c:'#10b981',e:'🟢'},'gpt-4o':{l:'GPT-4o',c:'#9ca3af',e:'⚫'},
  'erin':{l:'ERIN',c:'#ef4444',e:'🔴'},
}
function mi(s){
  if(!s) return {l:'AI',c:'#64748b',e:'🤖'}
  if(MI[s]) return MI[s]
  const k=Object.keys(MI).find(k=>s.includes(k.split('-')[0]))
  return k?MI[k]:{l:s,c:'#64748b',e:'🤖'}
}
function esc(s){return(s||'').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/\n/g,'<br>')}

// ROOMS
async function loadRooms(){
  try{rooms=await invoke('list_rooms')}catch(e){rooms=[]}
  renderRooms()
}
function renderRooms(){
  const el=document.getElementById('rl'); if(!el) return
  el.innerHTML=''
  if(!rooms.length){el.innerHTML='<div style="padding:16px;text-align:center;font-size:12px;color:var(--muted)">No rooms yet.<br>Click + to create one.</div>';return}
  rooms.forEach(r=>{
    const d=document.createElement('div')
    d.className='ri'+(activeRoom?.id===r.id?' act':'')
    const icon=r.mode==='parallel'?'⚡':r.mode==='roundrobin'?'🔄':'🤖'
    d.innerHTML=`<span>${icon} ${esc(r.id)}</span><span class="rd" data-id="${r.id}">✕</span>`
    d.addEventListener('click',()=>switchRoom(r))
    d.querySelector('.rd').addEventListener('click',async e=>{
      e.stopPropagation()
      await invoke('delete_room',{roomId:r.id}).catch(()=>{})
      if(activeRoom?.id===r.id){activeRoom=null;showEmpty()}
      loadRooms()
    })
    el.appendChild(d)
  })
}
async function switchRoom(r){
  activeRoom=r; renderRooms()
  showChat()
  document.getElementById('ct').textContent=r.id
  document.querySelectorAll('.mb').forEach(b=>b.classList.toggle('act',b.dataset.mode===r.mode))
  try{messages=await invoke('get_room_messages',{roomId:r.id})}catch(e){messages=[]}
  streamBuffers={}; renderMsgs()
}
function showEmpty(){
  document.getElementById('es').style.display='flex'
  document.getElementById('ch').style.display='none'
  document.getElementById('ms').style.display='none'
  document.getElementById('ia').style.display='none'
}
function showChat(){
  document.getElementById('es').style.display='none'
  document.getElementById('ch').style.display='flex'
  document.getElementById('ms').style.display='flex'
  document.getElementById('ia').style.display='block'
}

// NEW ROOM
document.getElementById('arb').addEventListener('click',()=>{
  const f=document.getElementById('nrf')
  f.style.display=f.style.display==='none'?'block':'none'
  if(f.style.display==='block') document.getElementById('nri').focus()
})
document.getElementById('cb').addEventListener('click',()=>{
  document.getElementById('nrf').style.display='block'
  const i=document.getElementById('nri'); i.value='conference-1'; i.focus()
})
document.getElementById('nri').addEventListener('keydown',async e=>{
  if(e.key==='Escape'){document.getElementById('nrf').style.display='none';return}
  if(e.key!=='Enter') return
  const name=e.target.value.trim(); if(!name) return
  e.target.value=''; document.getElementById('nrf').style.display='none'
  try{
    const statuses=await invoke('get_auth_status').catch(()=>[])
    const hk={}; statuses.forEach(s=>hk[s.provider_id]=s.has_key)
    const parts=[]
    const em=await gcfg('model-erin','erin'); parts.push(em)
    const cloud=[
      {id:'anthropic',fb:'claude-opus-4-5'},{id:'anthropic-web',fb:'claude-web'},
      {id:'xai',fb:'grok-3'},{id:'xai-web',fb:'grok-web'},
      {id:'google',fb:'gemini-2.5-pro'},{id:'openai',fb:'gpt-4o'},
      {id:'github-copilot',fb:'github/gpt-4o'},{id:'mistral',fb:'mistral-large-latest'},
    ]
    for(const cp of cloud){
      if(hk[cp.id]){const m=await gcfg('model-'+cp.id,cp.fb); if(!parts.includes(m)) parts.push(m)}
    }
    await invoke('create_room',{roomId:name,participants:parts,moderatorModel:em,mode:'moderator'})
    await loadRooms()
    const r=rooms.find(x=>x.id===name); if(r) switchRoom(r)
  }catch(err){alert('Error: '+err)}
})

// MODE BUTTONS
document.querySelectorAll('.mb').forEach(b=>{
  b.addEventListener('click',async()=>{
    if(!activeRoom) return
    document.querySelectorAll('.mb').forEach(x=>x.classList.remove('act'))
    b.classList.add('act'); activeRoom.mode=b.dataset.mode
    await invoke('set_room_mode',{roomId:activeRoom.id,mode:b.dataset.mode}).catch(()=>{})
  })
})

// MESSAGES
function renderMsgs(){
  const el=document.getElementById('ms'); if(!el) return
  el.innerHTML=''
  messages.forEach(m=>el.appendChild(buildMsg(m)))
  Object.entries(streamBuffers).forEach(([s,t])=>{if(t) el.appendChild(buildStream(s,t))})
  el.scrollTop=el.scrollHeight
}
function buildMsg(m){
  if(m.role==='user'||m.speaker==='user'){
    const d=document.createElement('div'); d.className='mu'
    d.innerHTML=`<div class="b">${esc(m.content)}</div>`; return d
  }
  if(!m.speaker||m.speaker==='System'||m.content?.startsWith('📋')){
    const d=document.createElement('div'); d.className='sy'
    d.textContent=(m.content||'').replace(/^📋 \*|\*$/g,''); return d
  }
  const i=mi(m.speaker); const d=document.createElement('div'); d.className='ma'
  d.innerHTML=`<div class="av" style="background:${i.c}22;border:1px solid ${i.c}44">${i.e}</div>
    <div class="ab"><div class="an" style="color:${i.c}">${esc(m.speaker)}</div>
    <div class="at">${esc(m.content)}</div></div>`; return d
}
function buildStream(spk,txt){
  const i=mi(spk); const d=document.createElement('div'); d.className='ma'
  d.innerHTML=`<div class="av" style="background:${i.c}22;border:1px solid ${i.c}44">${i.e}</div>
    <div class="ab"><div class="an" style="color:${i.c}">${esc(spk)}</div>
    <div class="at">${esc(txt)}<span class="cur"></span></div></div>`; return d
}

// SEND
const miEl=document.getElementById('mi'), sbEl=document.getElementById('sb'), xbEl=document.getElementById('xb')
miEl.addEventListener('keydown',e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();send()}})
sbEl.addEventListener('click',send)
xbEl.addEventListener('click',async()=>{
  if(activeRoom) await invoke('cancel_generation',{roomId:activeRoom.id}).catch(()=>{})
  setGen(false)
})
async function send(){
  const t=miEl.value.trim(); if(!t||isGenerating||!activeRoom) return
  miEl.value=''; setGen(true)
  messages.push({role:'user',content:t,speaker:'user',ts:Date.now()}); renderMsgs()
  try{await invoke('send_message',{roomId:activeRoom.id,content:t})}
  catch(e){messages.push({role:'assistant',content:'⚠️ '+e,speaker:'System'});setGen(false);renderMsgs()}
}
function setGen(v){
  isGenerating=v; sbEl.style.display=v?'none':'block'; xbEl.style.display=v?'block':'none'; miEl.disabled=v
}

// EVENTS
async function initEvents(){
  await listen('stream-delta',e=>{
    const{room_id,speaker,delta}=e.payload; if(room_id!==activeRoom?.id) return
    streamBuffers[speaker]=(streamBuffers[speaker]||'')+delta; renderMsgs()
  })
  await listen('stream-end',e=>{
    const{room_id,speaker}=e.payload; if(room_id!==activeRoom?.id) return
    delete streamBuffers[speaker]; renderMsgs()
  })
  await listen('turn-complete',async e=>{
    const{room_id}=e.payload; streamBuffers={}; setGen(false)
    if(room_id===activeRoom?.id){
      try{messages=await invoke('get_room_messages',{roomId:room_id})}catch(err){}
      renderMsgs()
    }
    loadRooms()
  })
  await listen('turn-error',e=>{
    streamBuffers={}; setGen(false)
    if(e.payload.room_id===activeRoom?.id){
      messages.push({role:'assistant',content:'⚠️ '+e.payload.error,speaker:'System'}); renderMsgs()
    }
  })
}

// CONFIG HELPERS
async function gcfg(key,def=''){
  try{const v=await invoke('get_config_value',{key});return(v&&v!=='null'&&v!=='')?v:def}catch(e){return def}
}
async function scfg(key,val){try{await invoke('save_config_value',{key,value:val})}catch(e){}}

// SETTINGS
async function loadProviderConfigs(){
  for(const p of PROVIDERS){
    const def=p.models?.length?p.models[0].id:''
    selectedModels[p.id]=await gcfg('model-'+p.id,def)
    if(p.configKeys) for(const ck of p.configKeys) configValues[ck.key]=await gcfg(ck.key,ck.def||'')
  }
}

async function loadSettings(){
  await loadProviderConfigs()
  try{
    const list=await invoke('get_auth_status')
    providerStatuses={}; list.forEach(s=>{providerStatuses[s.provider_id]=s})
  }catch(e){console.error('get_auth_status:',e)}
  try{
    const apps=await invoke('check_installed_apps')
    const banner=document.getElementById('impb'), btns=document.getElementById('imps')
    if((apps.claude_desktop||apps.grok_desktop)&&banner&&btns){
      banner.style.display='block'; btns.innerHTML=''
      if(apps.claude_desktop){
        const b=document.createElement('button'); b.className='btn bsm'
        b.style.background='#92400e'; b.style.color='#fff'
        b.textContent=(apps.claude_logged_in?'Re-import':'Import')+' 🟠 Claude Session'
        b.onclick=async()=>{
          const r=await invoke('import_claude_session')
          const el=document.getElementById('impr')
          if(el){el.textContent=r.message;el.style.color=r.success?'#10b981':'#ef4444'}
          loadSettings()
        }; btns.appendChild(b)
      }
      if(apps.grok_desktop){
        const b=document.createElement('button'); b.className='btn bsm'
        b.style.background='#3730a3'; b.style.color='#fff'
        b.textContent=(apps.grok_logged_in?'Re-import':'Import')+' 🟣 Grok Session'
        b.onclick=async()=>{
          const r=await invoke('import_grok_session')
          const el=document.getElementById('impr')
          if(el){el.textContent=r.message;el.style.color=r.success?'#10b981':'#ef4444'}
          loadSettings()
        }; btns.appendChild(b)
      }
    }
  }catch(e){}
  renderProviders()
}

function renderProviders(){
  const list=document.getElementById('pvl'); if(!list) return
  list.innerHTML=''
  for(const p of PROVIDERS){
    const s=providerStatuses[p.id]
    const hasKey=!!(p.noKey||s?.has_key)
    const card=document.createElement('div')
    card.className='pc'+(hasKey&&!p.noKey?' ok':'')+((!hasKey&&!p.noKey)?' dim':'')
    let badge=`<span class="pbg bn">No key</span>`
    if(p.noKey) badge=`<span class="pbg bl">Local</span>`
    else if(s?.has_key) badge=`<span class="pbg bk">✓ Set</span>`
    let modelHtml=''
    if(p.models&&p.models.length){
      const opts=p.models.map(m=>`<option value="${m.id}"${selectedModels[p.id]===m.id?' selected':''}>${m.label}</option>`).join('')
      modelHtml=`<div class="ps2"><span class="pl">Active model</span>
        <select class="pse msel" data-pid="${p.id}"${!hasKey?' disabled':''}>${opts}</select></div>`
    }
    let cfgHtml=''
    if(p.configKeys) cfgHtml=p.configKeys.map(ck=>`
      <div class="ps2"><span class="pl">${ck.label}</span>
        <div class="pr"><input class="pi2 cfgi" data-key="${ck.key}" value="${esc(configValues[ck.key]||ck.def||'')}" placeholder="${ck.placeholder}" style="font-family:monospace"/>
        <button class="btn bs bsm cfgs" data-key="${ck.key}">Set</button></div></div>`).join('')
    let authHtml=''
    if(!p.noKey&&!p.session){
      authHtml=`<div class="ps2"><span class="pl">API Key</span>
        <div class="pr">${p.consoleUrl?`<button class="btn bs bsm" onclick="window.__con('${p.id}')">🔑 Get Key</button>`:''}
          <input type="password" class="pi2 ki" id="ki-${p.id}" placeholder="${s?.has_key?'••••• (update)':(p.placeholder||'API key')}" style="font-family:monospace"/>
          <button class="btn bp bsm ks" data-pid="${p.id}">Save</button></div>
        ${s?.has_key?`<div class="pr" style="margin-top:8px;gap:8px">
          <button class="btn bs bsm kt" data-pid="${p.id}">🔌 Test</button>
          <button class="bd kd" data-pid="${p.id}">🗑 Remove</button></div>
          <div class="tr" id="tr-${p.id}"></div>`:''}
        </div>`
    }else if(p.session){
      authHtml=s?.has_key
        ?`<div class="ps2" style="display:flex;align-items:center;gap:12px">
            <span style="font-size:12px;color:var(--grn)">✅ Session active</span>
            <button class="btn bs bsm kt" data-pid="${p.id}">🔌 Test</button>
            <button class="bd kd" data-pid="${p.id}">🗑 Clear</button></div>
          <div class="tr" id="tr-${p.id}"></div>`
        :`<div style="font-size:12px;color:var(--muted);margin-top:8px">Use import button above ↑</div>`
    }else{
      authHtml=`<div class="ps2">
        <button class="btn bs bsm kt" data-pid="${p.id}">🔌 Test Connection</button>
        <div class="tr" id="tr-${p.id}"></div></div>`
    }
    card.innerHTML=`
      <div class="ph"><div class="pt2"><span class="pi">${p.icon}</span>
        <div><div class="pn">${p.label}</div><div class="psu">${p.subtitle}</div></div></div>${badge}</div>
      <div class="pno">${p.note}</div>${modelHtml}${cfgHtml}${authHtml}`
    list.appendChild(card)
    card.querySelectorAll('.msel').forEach(sel=>sel.addEventListener('change',async()=>{
      selectedModels[sel.dataset.pid]=sel.value; await scfg('model-'+sel.dataset.pid,sel.value)
    }))
    card.querySelectorAll('.cfgs').forEach(btn=>btn.addEventListener('click',async()=>{
      const key=btn.dataset.key, val=card.querySelector(`.cfgi[data-key="${key}"]`)?.value.trim()
      if(val===undefined) return; configValues[key]=val; await scfg(key,val)
      btn.textContent='✓'; setTimeout(()=>btn.textContent='Set',1500)
    }))
    card.querySelectorAll('.ks').forEach(btn=>btn.addEventListener('click',async()=>{
      const pid=btn.dataset.pid, val=document.getElementById('ki-'+pid)?.value.trim()
      if(!val) return
      try{await invoke('save_api_key',{providerId:pid,apiKey:val});await loadSettings()}
      catch(e){alert('Error: '+e)}
    }))
    card.querySelectorAll('.kt').forEach(btn=>btn.addEventListener('click',async()=>{
      const pid=btn.dataset.pid, el=document.getElementById('tr-'+pid)
      if(el){el.textContent='Testing…';el.className='tr'}
      try{const msg=await invoke('test_connection',{providerId:pid});if(el){el.textContent=msg;el.className='tr tok'}}
      catch(e){if(el){el.textContent=String(e);el.className='tr ter'}}
    }))
    card.querySelectorAll('.kd').forEach(btn=>btn.addEventListener('click',async()=>{
      await invoke('delete_api_key',{providerId:btn.dataset.pid}).catch(()=>{});await loadSettings()
    }))
  }
}
window.__con=async pid=>{try{await invoke('open_console',{providerId:pid})}catch(e){}}

// MCP
const MCPP=[
  {name:'Fetch/Web',cmd:'npx -y @modelcontextprotocol/server-fetch'},
  {name:'Filesystem',cmd:'npx -y @modelcontextprotocol/server-filesystem /'},
  {name:'Git',cmd:'npx -y @modelcontextprotocol/server-git'},
  {name:'Postgres',cmd:'npx -y @modelcontextprotocol/server-postgres postgresql://localhost/mydb'},
  {name:'ERIN Memory',cmd:'http://10.1.1.19:5456',type:'http'},
]
async function loadMCP(){
  let servers=[]
  try{const r=await invoke('get_config_value',{key:'mcp-servers'});if(r&&r!=='null') servers=JSON.parse(r)}catch(e){}
  const el=document.getElementById('mcpl'); if(!el) return; el.innerHTML=''
  const all=[{id:'local-system',name:'Local System (built-in)',type:'stdio',cmd:'node /opt/mcp-local-server/dist/index.js',enabled:true,builtin:true,desc:'File system, processes, Docker, cron'},...servers]
  all.forEach(s=>{
    const d=document.createElement('div'); d.className='pc ok'; d.style.maxWidth='700px'
    d.innerHTML=`<div class="ph"><div class="pt2"><span class="pi">${s.type==='http'?'🌐':'⚡'}</span>
      <div><div class="pn">${esc(s.name)}</div><div class="psu" style="font-family:monospace">${esc(s.desc||s.cmd||'')}</div></div></div>
      <div style="display:flex;align-items:center;gap:8px"><span class="pbg bk">${s.type.toUpperCase()}</span>
      ${!s.builtin?`<button class="bd" data-mid="${s.id}">✕</button>`:''}</div></div>`
    el.appendChild(d)
    d.querySelectorAll('[data-mid]').forEach(b=>b.addEventListener('click',async()=>{
      servers=servers.filter(x=>x.id!==b.dataset.mid); await scfg('mcp-servers',JSON.stringify(servers)); loadMCP()
    }))
  })
  const pp=document.getElementById('mpp')
  if(pp&&!pp.children.length) MCPP.forEach(p=>{
    const b=document.createElement('button'); b.className='btn bs bsm'; b.textContent='+ '+p.name
    b.onclick=async()=>{servers.push({id:'mcp-'+Date.now(),name:p.name,type:p.type||'stdio',cmd:p.cmd,enabled:true});await scfg('mcp-servers',JSON.stringify(servers));loadMCP()}
    pp.appendChild(b)
  })
}
document.getElementById('amb').addEventListener('click',()=>document.getElementById('amf').style.display='block')
document.getElementById('amc').addEventListener('click',()=>document.getElementById('amf').style.display='none')
document.getElementById('ams').addEventListener('click',async()=>{
  const name=document.getElementById('mn').value.trim(), type=document.getElementById('mt').value, cmd=document.getElementById('mc').value.trim()
  if(!name||!cmd) return
  const r=await gcfg('mcp-servers','[]'); const servers=JSON.parse(r)
  servers.push({id:'mcp-'+Date.now(),name,type,cmd,enabled:true}); await scfg('mcp-servers',JSON.stringify(servers))
  document.getElementById('amf').style.display='none'; document.getElementById('mn').value=''; document.getElementById('mc').value=''; loadMCP()
})

// INIT
async function init(){await initEvents();await loadRooms();if(rooms.length>0) switchRoom(rooms[0])}
init()
