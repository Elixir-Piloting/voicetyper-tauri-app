import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';

let config = {};
let modes = {};

async function loadConfig() {
  try {
    config = await invoke('get_config');
    modes = config.writing_modes || {};
    applyConfig();
  } catch (e) {
    console.error('load config:', e);
  }
}

function applyConfig() {
  document.getElementById('hk-dictation').textContent = config.hotkey || 'Ctrl+Super';
  document.getElementById('chk-ptt').checked = config.push_to_talk !== false;
  document.getElementById('inp-trigger').value = config.trigger_phrase || 'hey voicetyper';

  document.getElementById('inp-groqkey').value = config.groq_api_key || '';
  document.getElementById('inp-orkey').value = config.openrouter_key || '';

  const engine = config.use_groq ? 'groq' : 'local';
  document.querySelectorAll('input[name="stt-engine"]').forEach(el => {
    el.checked = el.value === engine;
  });
  toggleEngineFields(engine);

  document.getElementById('sel-model').value = config.whisper_model || 'small';
  document.getElementById('sel-ctype').value = config.whisper_compute_type || 'float32';
  document.getElementById('sel-lang').value = config.language || 'en';

  document.getElementById('sel-cl-engine').value = config.cleanup_engine || 'groq';
  document.getElementById('inp-groqcl-model').value = config.groq_cleanup_model || 'meta-llama/llama-4-scout-17b-16e-instruct';
  document.getElementById('inp-ormodel').value = config.openrouter_model || 'anthropic/claude-3.5-haiku';
  document.getElementById('inp-url').value = config.ollama_url || 'http://localhost:11434';
  document.getElementById('inp-ollama').value = config.ollama_model || 'qwen2.5:7b-instruct';

  document.getElementById('chk-auto').checked = config.writing_mode_auto !== false;
  buildModeTabs(modes, config.writing_mode || 'General');
  (config.replacements || []).forEach(r => addRep(r[0], r[1]));
  // Poll hotkey status
  invoke('get_hotkey_status').then(data => {
    showHotkeyStatus(data);
  }).catch(() => {});
}

function toggleEngineFields(engine) {
  const showLocal = engine === 'local';
  document.getElementById('row-whisper-model').style.display = showLocal ? 'flex' : 'none';
  document.getElementById('row-compute').style.display = showLocal ? 'flex' : 'none';
  document.getElementById('row-download').style.display = showLocal ? 'flex' : 'none';
}

document.querySelectorAll('input[name="stt-engine"]').forEach(el => {
  el.addEventListener('change', () => {
    if (el.checked) toggleEngineFields(el.value);
  });
});

function switchTab(name) {
  document.querySelectorAll('.stab').forEach(t => t.classList.toggle('active', t.dataset.tab === name));
  document.querySelectorAll('.tab-pane').forEach(p => p.classList.toggle('active', p.id === 'pane-' + name));
}

// Hotkey recorder
let recordingHotkey = false;
let hotkeyEl = null;
function recordHotkey(el) {
  if (recordingHotkey) return;
  recordingHotkey = true;
  hotkeyEl = el;
  el.classList.add('recording');
  el.textContent = 'press combo...';
  el.focus();
}

function captureHotkey(e) {
  if (!recordingHotkey) return;
  e.preventDefault();
  const parts = [];
  if (e.ctrlKey) parts.push('ctrl');
  if (e.altKey) parts.push('alt');
  if (e.shiftKey) parts.push('shift');
  if (e.metaKey) parts.push('super');
  if (e.key.startsWith('F') && e.key.length <= 3) {
    parts.push(e.key.toLowerCase());
  } else if (parts.length > 0 && e.key !== 'Control' && e.key !== 'Alt' && e.key !== 'Shift' && e.key !== 'Meta') {
    parts.push(e.key.toLowerCase());
  }
  if (parts.length < 2) return;
  recordingHotkey = false;
  if (hotkeyEl) {
    hotkeyEl.classList.remove('recording');
    hotkeyEl.textContent = parts.join('+');
    hotkeyEl = null;
  }
}

document.addEventListener('keydown', (e) => {
  if (recordingHotkey) captureHotkey(e);
});

// Model download
async function downloadModel() {
  const btn = document.getElementById('btn-download');
  const status = document.getElementById('dl-status');
  btn.disabled = true;
  status.textContent = 'downloading...';
  try {
    await invoke('download_whisper_model', {
      model: document.getElementById('sel-model').value
    });
    status.textContent = '✓ done';
  } catch (e) {
    status.textContent = '✗ ' + e;
  }
  btn.disabled = false;
}

// API key testing
async function testGroqKey() {
  const el = document.getElementById('result-groq');
  el.className = 'test-result';
  el.textContent = '…';
  try {
    const ok = await invoke('test_groq_key', { key: document.getElementById('inp-groqkey').value });
    el.textContent = ok ? '✓' : '✗';
    el.className = 'test-result ' + (ok ? 'ok' : 'fail');
  } catch (e) {
    el.textContent = '✗';
    el.className = 'test-result fail';
  }
}

async function testOpenRouterKey() {
  const el = document.getElementById('result-or');
  el.className = 'test-result';
  el.textContent = '…';
  try {
    const ok = await invoke('test_openrouter_key', { key: document.getElementById('inp-orkey').value });
    el.textContent = ok ? '✓' : '✗';
    el.className = 'test-result ' + (ok ? 'ok' : 'fail');
  } catch (e) {
    el.textContent = '✗';
    el.className = 'test-result fail';
  }
}

// Writing modes
let editingMode = '';

function buildModeTabs(modes, current) {
  const names = Object.keys(modes);
  const tabs = document.getElementById('mode-tabs');
  if (names.length === 0) {
    tabs.innerHTML = '';
    document.getElementById('mode-editor').innerHTML = '<p class="hint" style="padding:0">No custom modes. Click + Add Mode to create one.</p>';
    return;
  }
  tabs.innerHTML = names.map(n =>
    `<div class="st${n===current?' active':''}" onclick="switchModeTab('${n}')">${n}</div>`
  ).join('');
  if (!current || !modes[current]) current = names[0] || 'General';
  editingMode = current;
  const m = modes[current] || { match: {} };
  const match = m.match || {};
  const ed = document.getElementById('mode-editor');
  ed.innerHTML = `
    <div class="f-row"><label>Class</label><input id="me-class" type="text" value="${(match.class||'').replace(/"/g,'&quot;')}" placeholder="regex to match window class"></div>
    <div class="f-row"><label>Title</label><input id="me-title" type="text" value="${(match.title||'').replace(/"/g,'&quot;')}" placeholder="regex to match window title"></div>
  `;
}

function switchModeTab(name) {
  if (editingMode && modes[editingMode]) {
    const c = document.getElementById('me-class');
    const t = document.getElementById('me-title');
    if (c) { modes[editingMode].match = modes[editingMode].match || {}; modes[editingMode].match.class = c.value; }
    if (t) { modes[editingMode].match.title = t.value; }
  }
  buildModeTabs(modes, name);
}

function addMode() {
  const name = prompt('Mode name:');
  if (!name || modes[name]) return;
  modes[name] = { match: { class: '', title: '' } };
  buildModeTabs(modes, name);
}

// Replacements
function addRep(from, to) {
  const el = document.createElement('div');
  el.className = 'rep-row';
  el.innerHTML = `<input class="rep-from" type="text" placeholder="from" value="${from||''}"><input class="rep-to" type="text" placeholder="to" value="${to||''}"><button class="btn" onclick="this.parentElement.remove()">x</button>`;
  document.getElementById('replist').appendChild(el);
}

// Save
async function saveSettings() {
  if (editingMode && modes[editingMode]) {
    const c = document.getElementById('me-class');
    const t = document.getElementById('me-title');
    if (c) { modes[editingMode].match = modes[editingMode].match || {}; modes[editingMode].match.class = c.value; }
    if (t) { modes[editingMode].match.title = t.value; }
  }
  const reps = [];
  document.querySelectorAll('.rep-row').forEach(r => {
    const from = r.querySelector('.rep-from');
    const to = r.querySelector('.rep-to');
    if (from && to && from.value.trim()) reps.push([from.value.trim(), to.value]);
  });

  const payload = {
    hotkey: document.getElementById('hk-dictation').textContent,
    push_to_talk: document.getElementById('chk-ptt').checked,
    trigger_phrase: document.getElementById('inp-trigger').value,
    groq_api_key: document.getElementById('inp-groqkey').value,
    openrouter_key: document.getElementById('inp-orkey').value,
    use_groq: document.querySelector('input[name="stt-engine"]:checked').value === 'groq',
    whisper_model: document.getElementById('sel-model').value,
    whisper_compute_type: document.getElementById('sel-ctype').value,
    language: document.getElementById('sel-lang').value,
    cleanup_engine: document.getElementById('sel-cl-engine').value,
    groq_cleanup_model: document.getElementById('inp-groqcl-model').value,
    openrouter_model: document.getElementById('inp-ormodel').value,
    ollama_url: document.getElementById('inp-url').value,
    ollama_model: document.getElementById('inp-ollama').value,
    writing_mode_auto: document.getElementById('chk-auto').checked,
    writing_mode: editingMode,
    writing_modes: modes,
    replacements: reps,
  };

  try {
    await invoke('save_config', { config: payload });
    document.getElementById('save-msg').textContent = 'Saved!';
    setTimeout(() => document.getElementById('save-msg').textContent = '', 2000);
  } catch (e) {
    document.getElementById('save-msg').textContent = 'Error: ' + e;
  }
}

function closeSettings() {
  getCurrentWindow().close();
}

// Transcription results
let lastRaw = '';
let lastCleaned = '';

async function copyResult(type) {
  const text = type === 'raw' ? lastRaw : lastCleaned;
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch (_) {
    // fallback
  }
}

async function pasteResult() {
  if (!lastCleaned) return;
  try {
    await invoke('paste_text', { text: lastCleaned });
  } catch (e) {
    console.error('paste:', e);
  }
}

async function retryCleanup() {
  if (!lastRaw) return;
  const el = document.getElementById('result-cleaned');
  el.textContent = '…';
  try {
    const cleaned = await invoke('retry_cleanup', { text: lastRaw });
    lastCleaned = cleaned;
    el.textContent = cleaned || '(empty)';
  } catch (e) {
    el.textContent = 'Error: ' + e;
  }
}

// Expose functions to window for inline onclick handlers
window.switchTab = switchTab;
window.recordHotkey = recordHotkey;
window.captureHotkey = captureHotkey;
window.downloadModel = downloadModel;
window.testGroqKey = testGroqKey;
window.testOpenRouterKey = testOpenRouterKey;
window.saveSettings = saveSettings;
window.closeSettings = closeSettings;
window.addMode = addMode;
window.switchModeTab = switchModeTab;
window.addRep = addRep;
window.copyResult = copyResult;
window.pasteResult = pasteResult;
window.retryCleanup = retryCleanup;
window.toggleRecord = toggleRecord;

// Init
listen('transcription-result', (event) => {
  const data = event.payload;
  lastRaw = data.raw || '';
  lastCleaned = data.cleaned || '';
  const rawEl = document.getElementById('result-raw');
  const cleanedEl = document.getElementById('result-cleaned');
  if (rawEl) rawEl.textContent = lastRaw || '(empty)';
  if (cleanedEl) cleanedEl.textContent = lastCleaned || '(empty)';
});

listen('recording-status', (event) => {
  const data = event.payload;
  const el = document.getElementById('recording-status');
  const dot = document.getElementById('status-dot');
  const text = document.getElementById('status-text');
  const btn = document.getElementById('btn-record');
  if (!el || !dot || !text || !btn) return;
  if (data.recording) {
    el.style.display = 'flex';
    dot.className = 'status-dot recording';
    text.textContent = 'Recording...';
    btn.textContent = 'Stop Recording';
  } else if (data.processing) {
    el.style.display = 'flex';
    dot.className = 'status-dot processing';
    text.textContent = 'Processing...';
    btn.textContent = 'Recording...';
    btn.disabled = true;
  } else {
    el.style.display = 'none';
    btn.textContent = 'Start Recording';
    btn.disabled = false;
  }
});

listen('hotkey-status', (event) => {
  showHotkeyStatus(event.payload);
});

function showHotkeyStatus(data) {
  const el = document.getElementById('hk-status');
  if (!el) return;
  const status = data.status;
  if (status === 'registered') {
    el.innerHTML = '<span class="hk-ok">✓ Registered (' + (data.hotkey || 'ctrl+super') + ')</span>';
  } else if (status === 'failed') {
    el.innerHTML = '<span class="hk-fail">✗ Registration failed — check terminal logs</span>';
  } else {
    el.textContent = status === 'waiting' ? 'registering...' : (status || 'unknown');
  }
  const rec = document.getElementById('hk-dictation');
  if (rec && data.hotkey) rec.textContent = data.hotkey;
}

async function toggleRecord() {
  try {
    await invoke('toggle_recording');
  } catch (e) {
    console.error('toggle:', e);
  }
}

// Init state check
document.addEventListener('DOMContentLoaded', loadConfig);
