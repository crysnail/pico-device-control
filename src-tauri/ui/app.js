// ======================== PicoManager v2.0 现代一体化前端交互 ========================

const IS_TAURI_ENV = (
  window.location.protocol === 'tauri:' ||
  window.location.protocol === 'asset:' ||
  window.location.hostname === 'tauri.localhost' ||
  Boolean(window.__TAURI__)
);

let LOCAL_PORT = 8999;
let RUNTIME_PORT = 8999;
let CONN_MODE = 'local'; // 'local' | 'remote'
let REMOTE_HOST = '';
let REMOTE_PORT = 8999;
let API_BASE = IS_TAURI_ENV ? `http://127.0.0.1:${LOCAL_PORT}` : '';
let PRIMARY_LAN_URL = '';

function updateConnectionBase() {
  if (CONN_MODE === 'remote' && REMOTE_HOST) {
    RUNTIME_PORT = REMOTE_PORT || 8999;
    API_BASE = `http://${REMOTE_HOST}:${RUNTIME_PORT}`;
  } else {
    RUNTIME_PORT = LOCAL_PORT;
    API_BASE = IS_TAURI_ENV ? `http://127.0.0.1:${LOCAL_PORT}` : '';
  }
  updateConnModeUi();
}

function updateConnModeUi() {
  const pill = document.getElementById('conn-mode-pill');
  const txt = document.getElementById('conn-mode-text');
  if (pill && txt) {
    if (CONN_MODE === 'remote' && REMOTE_HOST) {
      pill.className = 'status-pill status-pill--online';
      txt.textContent = `远程服务: ${REMOTE_HOST}:${REMOTE_PORT}`;
    } else {
      pill.className = 'status-pill status-pill--info';
      txt.textContent = '当前模式: 本地直连';
    }
  }
  if (CONN_MODE === 'remote' && REMOTE_HOST) {
    safeSetText('header-port-desc', `远程服务模式 · 已连接 ${REMOTE_HOST}:${REMOTE_PORT}`);
  } else {
    safeSetText('header-port-desc', `Rust 原生一体化控制中心 · HTTP ${LOCAL_PORT} / UDP 8998`);
  }
}

function apiUrl(path) {
  if (path && path.startsWith('/') && API_BASE) {
    return API_BASE + path;
  }
  return path;
}

function wsUrl(path) {
  if (API_BASE) {
    return API_BASE.replace(/^http/, 'ws') + path;
  }
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${window.location.host}${path}`;
}

const originalFetch = window.fetch.bind(window);
window.fetch = function(url, init) {
  if (typeof url === 'string' && url.startsWith('/') && API_BASE) {
    url = API_BASE + url;
  }
  return originalFetch(url, init);
};

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function safeSetText(id, text) {
  const el = document.getElementById(id);
  if (el) el.textContent = text;
}

async function copyTextToClipboard(text) {
  const val = String(text ?? '');
  if (!val) return false;
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(val);
      return true;
    }
  } catch { }
  try {
    const ta = document.createElement('textarea');
    ta.value = val;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    ta.style.pointerEvents = 'none';
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}

// ======================== 全局状态 ========================
const devices = new Map();
let projects = [];
let systemConfig = null;
const selectedSns = new Set();
let currentActiveGroupId = 'all';
let currentMonitorSn = null;
let currentProject = null;
let globalVolume = 70;
let pendingSystemAction = null;
let realtimeSocket = null;
let wsReconnectTimer = null;
let wsReconnectAttempts = 0;
let isStreamingActive = false;
let snapshotTimer = null;
let streamStatusPollTimer = null;
let lastKnownSuiteHealth = null;
let adbDevicesList = [];
let deployLogLines = 0;
let isDeployLogExpanded = false;
let toastTimer = null;

function isDesktopApp(app) {
  if (!app) return true;
  const a = app.trim();
  return a === '' ||
    a === '系统桌面' ||
    a === 'com.picovr.enterpriseassistant' ||
    a === 'com.pico.launcher';
}

const WALLPAPERS = [
  { name: '大乱斗', url: './wallpaper/' + encodeURIComponent('大乱斗.png') },
  { name: '大冒险', url: './wallpaper/' + encodeURIComponent('大冒险.png') },
  { name: '大师版', url: './wallpaper/' + encodeURIComponent('大师版.png') },
  { name: '王城', url: './wallpaper/' + encodeURIComponent('王城.png') }
];
let currentWallpaperIdx = 0;
let activeSlideSlot = 0;
let wallpaperTimer = null;

function matchWallpaperForProject(proj) {
  const txt = `${proj?.name || ''} ${proj?.packageName || ''}`.toLowerCase();
  if (txt.includes('大冒险') || txt.includes('ocean') || txt.includes('dino')) return WALLPAPERS[1];
  if (txt.includes('大师') || txt.includes('master')) return WALLPAPERS[2];
  if (txt.includes('王城') || txt.includes('wangcheng')) return WALLPAPERS[3];
  return WALLPAPERS[0];
}

function setWallpaperByUrl(url) {
  const s0 = document.getElementById('wp-slide-0');
  const s1 = document.getElementById('wp-slide-1');
  if (!s0 || !s1) return;
  const nextSlot = activeSlideSlot === 0 ? 1 : 0;
  const nextEl = nextSlot === 0 ? s0 : s1;
  const curEl = activeSlideSlot === 0 ? s0 : s1;
  nextEl.style.backgroundImage = `url("${url}")`;
  nextEl.classList.add('active');
  curEl.classList.remove('active');
  activeSlideSlot = nextSlot;
}

function initWallpapers() {
  const s0 = document.getElementById('wp-slide-0');
  if (s0 && WALLPAPERS[0]) {
    s0.style.backgroundImage = `url("${WALLPAPERS[0].url}")`;
    s0.classList.add('active');
  }
  if (wallpaperTimer) clearInterval(wallpaperTimer);
  wallpaperTimer = setInterval(() => {
    currentWallpaperIdx = (currentWallpaperIdx + 1) % WALLPAPERS.length;
    setWallpaperByUrl(WALLPAPERS[currentWallpaperIdx].url);
  }, 15000);
}

function cycleWallpaper() {
  currentWallpaperIdx = (currentWallpaperIdx + 1) % WALLPAPERS.length;
  const wp = WALLPAPERS[currentWallpaperIdx];
  setWallpaperByUrl(wp.url);
  showToast(`已切换壁纸: ${wp.name}`, '🖼️');
}

// ======================== 主题切换与界面预缩放 ========================
let currentThemeMode = localStorage.getItem('pico-theme-mode') || localStorage.getItem('pico-theme') || 'dark';
let currentUiScale = parseFloat(localStorage.getItem('pico-ui-scale') || '1.0') || 1.0;
let currentFontScale = parseFloat(localStorage.getItem('pico-font-scale') || '1.0') || 1.0;

function initTheme() {
  applyThemeMode(currentThemeMode, false);
  initDisplayScale();
  try {
    window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
      if (currentThemeMode === 'system') applyThemeMode('system', false);
    });
  } catch { }
}

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  const btn = document.getElementById('theme-toggle-btn');
  if (btn) btn.textContent = theme === 'dark' ? '☀️' : '🌙';
}

function applyThemeMode(mode, save = true) {
  currentThemeMode = mode || 'dark';
  if (save) {
    localStorage.setItem('pico-theme-mode', currentThemeMode);
  }
  let resolved = currentThemeMode;
  if (currentThemeMode === 'system') {
    resolved = window.matchMedia && window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  }
  localStorage.setItem('pico-theme', resolved);
  applyTheme(resolved);
  updateScaleControlsUI();
}

function toggleTheme() {
  const cur = document.documentElement.getAttribute('data-theme') || 'dark';
  const next = cur === 'dark' ? 'light' : 'dark';
  applyThemeMode(next, true);
}

function initDisplayScale() {
  setUiScale(currentUiScale, false);
  setFontScale(currentFontScale, false);
}

function openDisplaySettingsModal() {
  updateScaleControlsUI();
  document.getElementById('display-settings-modal')?.classList.remove('hidden');
}

function setUiScale(scale, save = true) {
  const num = parseFloat(scale);
  if (!Number.isFinite(num)) return;
  currentUiScale = Math.min(Math.max(num, 0.75), 1.35);
  if (save) localStorage.setItem('pico-ui-scale', String(currentUiScale));
  // 仅缩放前景顶栏与工作区，绝不缩放 html 根节点，确保底层 .wallpaper-bg 壁纸在任意缩放下始终 100% 满屏铺满
  document.documentElement.style.zoom = '';
  const topbar = document.querySelector('.topbar');
  const workspace = document.querySelector('.workspace');
  if (topbar) topbar.style.zoom = String(currentUiScale);
  if (workspace) workspace.style.zoom = String(currentUiScale);
  updateScaleControlsUI();
}

function onUiScaleSliderInput(val) {
  setUiScale(parseInt(val, 10) / 100, true);
}

function setFontScale(scale, save = true) {
  const num = parseFloat(scale);
  if (!Number.isFinite(num)) return;
  currentFontScale = Math.min(Math.max(num, 0.8), 1.4);
  if (save) localStorage.setItem('pico-font-scale', String(currentFontScale));
  document.documentElement.style.setProperty('--font-scale', String(currentFontScale));
  updateScaleControlsUI();
}

function onFontScaleSliderInput(val) {
  setFontScale(parseInt(val, 10) / 100, true);
}

function updateScaleControlsUI() {
  const pct = Math.round(currentUiScale * 100);
  const uiSlider = document.getElementById('ui-scale-slider');
  const uiDisplay = document.getElementById('ui-scale-display');
  const scaleLabel = document.getElementById('scale-label');
  if (uiSlider) uiSlider.value = String(pct);
  if (uiDisplay) uiDisplay.textContent = `${pct}%`;
  if (scaleLabel) scaleLabel.textContent = `缩放: ${pct}%`;

  const fontPct = Math.round(currentFontScale * 100);
  const fontSlider = document.getElementById('font-scale-slider');
  const fontDisplay = document.getElementById('font-scale-display');
  if (fontSlider) fontSlider.value = String(fontPct);
  if (fontDisplay) {
    if (fontPct <= 85) fontDisplay.textContent = `小 (${fontPct}%)`;
    else if (fontPct <= 105) fontDisplay.textContent = `标准 (${fontPct}%)`;
    else if (fontPct <= 120) fontDisplay.textContent = `大 (${fontPct}%)`;
    else fontDisplay.textContent = `特大 (${fontPct}%)`;
  }

  ['system', 'light', 'dark'].forEach((m) => {
    const el = document.getElementById(`theme-opt-${m}`);
    if (el) el.classList.toggle('btn--primary', currentThemeMode === m);
  });
}

// ======================== 弹窗与统一事件委托（100% 兼容 Tauri v2 严格 CSP） ========================
function closeModalById(id) {
  const el = document.getElementById(id);
  if (el) el.classList.add('hidden');
  if (id === 'confirm-modal') pendingSystemAction = null;
}

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    ['confirm-modal', 'alias-modal', 'suite-diag-modal', 'ota-modal', 'group-modal', 'adb-modal', 'config-modal', 'conn-mode-modal', 'display-settings-modal']
      .forEach((id) => closeModalById(id));
  }
});

document.addEventListener('click', (e) => {
  // 1. 点击弹窗遮罩层关闭
  const backdropId = e.target?.dataset?.backdropClose;
  if (backdropId) {
    closeModalById(backdropId);
    return;
  }

  // 2. 统一 data-action 按钮与卡片操作分发
  const trigger = e.target.closest('[data-action]');
  if (!trigger) return;

  if (trigger.dataset.stop === '1') {
    e.stopPropagation();
  }

  const action = trigger.dataset.action;
  const arg = trigger.dataset.arg ?? '';
  const arg2 = trigger.dataset.arg2 ?? '';

  switch (action) {
    case 'copyLanUrl': copyLanUrl(); break;
    case 'openDisplaySettingsModal': openDisplaySettingsModal(); break;
    case 'setUiScale': setUiScale(arg); break;
    case 'setFontScale': setFontScale(arg); break;
    case 'applyThemeMode': applyThemeMode(arg); break;
    case 'toggleTheme': toggleTheme(); break;
    case 'openAdbModal': openAdbModal(); break;
    case 'openSuiteDiagModal': openSuiteDiagModal(); break;
    case 'openOtaModal': openOtaModal(); break;
    case 'openConfigModal': openConfigModal(arg || 'app'); break;
    case 'openConnModeModal': openConnModeModal(); break;
    case 'switchToLocalMode': switchToLocalMode(); break;
    case 'connectRemoteService': connectRemoteService(); break;
    case 'selectRemoteHistory': {
      const [h, p] = arg.split(':');
      const ipEl = document.getElementById('conn-remote-ip');
      const portEl = document.getElementById('conn-remote-port');
      if (ipEl && h) ipEl.value = h;
      if (portEl && p) portEl.value = p;
      break;
    }
    case 'openGroupManageModal': openGroupManageModal(); break;
    case 'closeModal': closeModalById(arg); break;
    case 'closeConfirmModal': closeConfirmModal(); break;
    case 'dismissLowBatteryAlert':
      document.getElementById('low-battery-alert')?.classList.add('hidden');
      break;
    case 'selectAllOnline': selectAllOnline(); break;
    case 'selectCurrentGroup': selectCurrentGroup(); break;
    case 'selectAllIdle': selectAllIdle(); break;
    case 'selectAllRunning': selectAllRunning(); break;
    case 'clearSelection': clearSelection(); break;
    case 'batchAction': batchAction(arg); break;
    case 'promptBatchSystem': promptBatchSystem(arg); break;
    case 'executeBatchSystemConfirmed': executeBatchSystemConfirmed(); break;
    case 'copyCurrentSn': copyCurrentSn(); break;
    case 'toggleLiveStream': toggleLiveStream(); break;
    case 'singleDeviceAction': singleDeviceAction(arg); break;
    case 'openCurrentAliasModal': openCurrentAliasModal(); break;
    case 'switchConfigTab': switchConfigTab(arg); break;
    case 'openAddProjectForm': openAddProjectForm(); break;
    case 'openEditProjectForm': openEditProjectForm(arg); break;
    case 'openConfigAndEditProject':
      openConfigModal('app');
      openEditProjectForm(arg);
      break;
    case 'closeProjectForm': closeProjectForm(); break;
    case 'submitProjectForm': submitProjectForm(); break;
    case 'deleteProject': deleteProject(arg); break;
    case 'openBindHeadsetForm': openBindHeadsetForm(); break;
    case 'closeBindHeadsetForm': closeBindHeadsetForm(); break;
    case 'submitBindHeadsetForm': submitBindHeadsetForm(); break;
    case 'openAliasModal': openAliasModal(arg, arg2); break;
    case 'submitAliasModal': submitAliasModal(); break;
    case 'unbindHeadset': unbindHeadset(arg); break;
    case 'sendTestUdpMessage': sendTestUdpMessage(); break;
    case 'setPicoServiceMode': setPicoServiceMode(arg === 'true'); break;
    case 'restartPicoService': restartPicoService(); break;
    case 'loadPicoServiceStatus': loadPicoServiceStatus(); break;
    case 'switchAdbTab': switchAdbTab(arg); break;
    case 'startAdbScan': startAdbScan(); break;
    case 'loadAdbDevices': loadAdbDevices(); break;
    case 'syncAdbDevicesToConfig': syncAdbDevicesToConfig(); break;
    case 'connectManualAdb': connectManualAdb(); break;
    case 'readAdbSn': readAdbSn(arg); break;
    case 'rebootAdb': rebootAdb(arg); break;
    case 'disconnectAdb': disconnectAdb(arg); break;
    case 'toggleSelectAllDeployDevices': toggleSelectAllDeployDevices(arg === 'true'); break;
    case 'executeDeployInstallApk': executeDeployInstallApk(); break;
    case 'setDeployRemotePath': {
      const el = document.getElementById('deploy-push-remote');
      if (el) el.value = arg;
      break;
    }
    case 'executeDeployPushFile': executeDeployPushFile(); break;
    case 'executeDeployCreateFolder': executeDeployCreateFolder(); break;
    case 'queryDeployPackages': queryDeployPackages(); break;
    case 'executeDeployUninstallApp': executeDeployUninstallApp(); break;
    case 'toggleDeployLogHeight': toggleDeployLogHeight(); break;
    case 'copyDeployLog': copyDeployLog(); break;
    case 'clearDeployLog': clearDeployLog(); break;
    case 'openNewGroupForm': openNewGroupForm(); break;
    case 'editGroup': editGroup(arg); break;
    case 'deleteGroup': deleteGroup(arg); break;
    case 'cancelGroupForm': cancelGroupForm(); break;
    case 'saveGroupFromForm': saveGroupFromForm(); break;
    case 'toggleAllGroupDevices': toggleAllGroupDevices(arg === 'true'); break;
    case 'reconnectSuiteSdk': reconnectSuiteSdk(); break;
    case 'toggleDeviceSelect': toggleDeviceSelect(arg); break;
    case 'selectMonitorDevice': selectMonitorDevice(arg); break;
    default: break;
  }
});

document.addEventListener('change', (e) => {
  const el = e.target;
  const action = el?.dataset?.change;
  if (!action) return;
  switch (action) {
    case 'applyGlobalVolume': applyGlobalVolume(el.value, true); break;
    case 'onTestUdpActionChange': onTestUdpActionChange(el.value); break;
    case 'saveDeviceProject': saveDeviceProject(el.dataset.arg || '', el.value); break;
    case 'updateDeploySelectedCount': updateDeploySelectedCount(); break;
    case 'toggleAllAdbSyncSelect': toggleAllAdbSyncSelect(el.checked); break;
    case 'selectDeployUninstallPkg':
      if (el.value) {
        const input = document.getElementById('deploy-uninstall-pkg');
        if (input) input.value = el.value;
      }
      break;
    default: break;
  }
});

document.addEventListener('input', (e) => {
  const el = e.target;
  const action = el?.dataset?.input;
  if (!action) return;
  switch (action) {
    case 'onGlobalVolumeInput': onGlobalVolumeInput(el.value); break;
    case 'updateTestUdpPreview': updateTestUdpPreview(); break;
    case 'onUiScaleSliderInput': onUiScaleSliderInput(el.value); break;
    case 'onFontScaleSliderInput': onFontScaleSliderInput(el.value); break;
    default: break;
  }
});

// ======================== Toast & 轻量音效 ========================
let audioCtx = null;
function playSound(type = 'click') {
  try {
    if (!audioCtx) audioCtx = new (window.AudioContext || window.webkitAudioContext)();
    if (audioCtx.state === 'suspended') audioCtx.resume();
    const osc = audioCtx.createOscillator();
    const gain = audioCtx.createGain();
    osc.connect(gain);
    gain.connect(audioCtx.destination);
    const now = audioCtx.currentTime;
    if (type === 'click') {
      osc.frequency.setValueAtTime(760, now);
      osc.frequency.exponentialRampToValueAtTime(420, now + 0.045);
      gain.gain.setValueAtTime(0.08, now);
      gain.gain.exponentialRampToValueAtTime(0.005, now + 0.045);
      osc.start(now);
      osc.stop(now + 0.045);
    } else if (type === 'success') {
      osc.frequency.setValueAtTime(523.25, now);
      osc.frequency.setValueAtTime(659.25, now + 0.07);
      osc.frequency.setValueAtTime(783.99, now + 0.14);
      gain.gain.setValueAtTime(0.1, now);
      gain.gain.exponentialRampToValueAtTime(0.005, now + 0.24);
      osc.start(now);
      osc.stop(now + 0.24);
    } else if (type === 'warning') {
      osc.type = 'sawtooth';
      osc.frequency.setValueAtTime(300, now);
      osc.frequency.setValueAtTime(240, now + 0.09);
      gain.gain.setValueAtTime(0.08, now);
      gain.gain.exponentialRampToValueAtTime(0.005, now + 0.2);
      osc.start(now);
      osc.stop(now + 0.2);
    }
  } catch { }
}

function showToast(msg, icon = '✓') {
  const toast = document.getElementById('toast');
  const iconEl = document.getElementById('toast-icon');
  const msgEl = document.getElementById('toast-msg');
  if (!toast || !msgEl) return;
  if (iconEl) iconEl.textContent = icon === 'warning' ? '⚠️' : (icon === 'info' ? 'ℹ️' : icon);
  msgEl.textContent = msg;
  toast.classList.add('toast--show');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove('toast--show'), 3200);
}

// ======================== 初始化 ========================
async function resolveRuntimeConfig() {
  if (IS_TAURI_ENV) {
    try {
      const invoke = window.__TAURI__?.core?.invoke;
      if (typeof invoke === 'function') {
        const info = await invoke('get_runtime_info');
        if (info && info.webPort) {
          LOCAL_PORT = info.webPort;
        }
      }
    } catch (e) {
      console.warn('get_runtime_info fallback:', e);
    }
  }

  // 读取用户上次保存的连接模式（local / remote）
  const savedMode = localStorage.getItem('pico-conn-mode');
  const savedHost = localStorage.getItem('pico-remote-host') || '';
  const savedPort = parseInt(localStorage.getItem('pico-remote-port') || '8999', 10) || 8999;
  if (savedMode === 'remote' && savedHost) {
    CONN_MODE = 'remote';
    REMOTE_HOST = savedHost;
    REMOTE_PORT = savedPort;
  } else {
    CONN_MODE = 'local';
  }
  updateConnectionBase();

  try {
    const res = await fetch('/api/system/network');
    if (res.ok) {
      const net = await res.json();
      if (net.primaryUrl) {
        PRIMARY_LAN_URL = net.primaryUrl;
        const pill = document.getElementById('lan-url-pill');
        safeSetText('lan-url-text', net.primaryUrl.replace(/^http:\/\//, ''));
        if (pill) pill.classList.remove('hidden');
      }
    }
  } catch { }
}

function openConnModeModal() {
  safeSetText('conn-local-port', String(LOCAL_PORT));
  const ipEl = document.getElementById('conn-remote-ip');
  const portEl = document.getElementById('conn-remote-port');
  if (ipEl) ipEl.value = REMOTE_HOST || localStorage.getItem('pico-remote-host') || '';
  if (portEl) portEl.value = String(REMOTE_PORT || 8999);
  const st = document.getElementById('conn-remote-status');
  if (st) st.classList.add('hidden');
  renderConnHistory();
  document.getElementById('conn-mode-modal')?.classList.remove('hidden');
}

function renderConnHistory() {
  const wrap = document.getElementById('conn-history-wrap');
  if (!wrap) return;
  let history = [];
  try {
    history = JSON.parse(localStorage.getItem('pico-remote-history') || '[]');
  } catch { history = []; }
  if (!Array.isArray(history) || history.length === 0) {
    wrap.classList.add('hidden');
    wrap.innerHTML = '';
    return;
  }
  wrap.classList.remove('hidden');
  wrap.innerHTML = '<span class="panel-sub">最近连接:</span>' + history.map(item =>
    `<button type="button" class="btn btn--xs mono" data-action="selectRemoteHistory" data-arg="${escapeHtml(item)}">${escapeHtml(item)}</button>`
  ).join('');
}

function resetAndReconnectWorkspace() {
  if (isStreamingActive && currentMonitorSn) {
    stopLiveStreamInternal(currentMonitorSn);
  }
  devices.clear();
  selectedSns.clear();
  currentMonitorSn = null;
  if (realtimeSocket) {
    try { realtimeSocket.close(); } catch { }
    realtimeSocket = null;
  }
  loadConfig();
  loadDevices();
  initRealtimeSocket();
  checkSuiteHealthBackground();
}

async function switchToLocalMode() {
  CONN_MODE = 'local';
  localStorage.setItem('pico-conn-mode', 'local');
  updateConnectionBase();
  closeModalById('conn-mode-modal');
  resetAndReconnectWorkspace();
  showToast(`已切换至当前本地模式 (127.0.0.1:${LOCAL_PORT})`, '💻');
}

async function connectRemoteService() {
  const ip = (document.getElementById('conn-remote-ip')?.value || '').trim();
  const port = parseInt(document.getElementById('conn-remote-port')?.value || '8999', 10) || 8999;
  const statusEl = document.getElementById('conn-remote-status');
  const btn = document.getElementById('btn-connect-remote-mode');

  if (!ip) {
    showToast('请输入远程服务端 IP 地址', '⚠️');
    return;
  }

  if (btn) btn.disabled = true;
  if (statusEl) {
    statusEl.className = 'panel-sub text-accent';
    statusEl.textContent = `⏳ 正在探测 http://${ip}:${port}/api/system/health ...`;
    statusEl.classList.remove('hidden');
  }

  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new Error('连接超时（6秒内未收到响应）')), 6000);
    const res = await originalFetch(`http://${ip}:${port}/api/system/health`, { signal: controller.signal });
    clearTimeout(timer);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);

    CONN_MODE = 'remote';
    REMOTE_HOST = ip;
    REMOTE_PORT = port;
    localStorage.setItem('pico-conn-mode', 'remote');
    localStorage.setItem('pico-remote-host', ip);
    localStorage.setItem('pico-remote-port', String(port));

    // 记录最近连接历史
    let history = [];
    try { history = JSON.parse(localStorage.getItem('pico-remote-history') || '[]'); } catch { history = []; }
    const entry = `${ip}:${port}`;
    history = [entry, ...history.filter(x => x !== entry)].slice(0, 4);
    localStorage.setItem('pico-remote-history', JSON.stringify(history));

    updateConnectionBase();
    closeModalById('conn-mode-modal');
    resetAndReconnectWorkspace();
    showToast(`已连接远程服务 ${ip}:${port}`, '🌐');
  } catch (err) {
    if (statusEl) {
      statusEl.className = 'panel-sub text-rose';
      const detail = err?.name === 'AbortError'
        ? '连接超时（6秒内未收到对方响应）'
        : (err?.message || '网络不可达或服务未开启');
      statusEl.textContent = `❌ 连接失败 (${detail})，请确认对方机器已启动 PicoManager.exe、处于同一局域网且防火墙放行 ${port} 端口`;
      statusEl.classList.remove('hidden');
    }
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function copyLanUrl() {
  if (!PRIMARY_LAN_URL) return;
  const ok = await copyTextToClipboard(PRIMARY_LAN_URL);
  showToast(ok ? `已复制平板局域网地址: ${PRIMARY_LAN_URL}` : `复制失败，请手动复制: ${PRIMARY_LAN_URL}`, ok ? '📡' : '⚠️');
}

window.addEventListener('DOMContentLoaded', async () => {
  initTheme();
  initWallpapers();
  await resolveRuntimeConfig();
  await loadConfig();
  await loadDevices();
  initRealtimeSocket();
  setInterval(loadDevices, 2500);
  setInterval(loadConfig, 15000);
  startCanvasSimulation();
  checkSuiteHealthBackground();
  setInterval(checkSuiteHealthBackground, 5000);
});

// ======================== WebSocket 实时通道 ========================
function setSystemStatusPill(variant, text) {
  const pill = document.getElementById('system-status-pill');
  const txt = document.getElementById('system-status-text');
  if (!pill || !txt) return;
  pill.className = `status-pill status-pill--${variant}`;
  txt.textContent = text;
}

function initRealtimeSocket() {
  if (wsReconnectTimer) {
    clearTimeout(wsReconnectTimer);
    wsReconnectTimer = null;
  }
  if (realtimeSocket && (realtimeSocket.readyState === WebSocket.OPEN || realtimeSocket.readyState === WebSocket.CONNECTING)) {
    return;
  }
  if (typeof WebSocket === 'undefined') {
    setSystemStatusPill('warn', 'HTTP 轮询模式');
    return;
  }

  try {
    realtimeSocket = new WebSocket(wsUrl('/ws/pico'));
    realtimeSocket.onopen = () => {
      wsReconnectAttempts = 0;
      setSystemStatusPill('online', `后台在线 · ${RUNTIME_PORT}`);
      loadDevices();
    };
    realtimeSocket.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);
        if (!msg || !msg.type) return;
        if (msg.type === 'DeviceAdded' || msg.type === 'DeviceUpdated') {
          const dev = msg.data;
          if (dev && dev.sn) {
            devices.set(dev.sn, dev);
            renderMatrix();
            updateTopStats();
            if (currentMonitorSn === dev.sn) refreshMonitorTelemetry(dev);
          }
        } else if (msg.type === 'DeviceRemoved') {
          const sn = msg.data;
          if (sn) {
            devices.delete(sn);
            selectedSns.delete(sn);
            renderMatrix();
            updateTopStats();
            if (currentMonitorSn === sn) {
              const first = Array.from(devices.values())[0];
              if (first) selectMonitorDevice(first.sn);
            }
          }
        } else if (msg.type === 'UdpCommandReceived') {
          const info = msg.data || {};
          playSound(info.success ? 'success' : 'warning');
          showToast(`[UDP] ${info.message || ''}`, info.success ? '📡' : '⚠️');
        }
      } catch { }
    };
    realtimeSocket.onerror = () => setSystemStatusPill('warn', '实时通道重连中...');
    realtimeSocket.onclose = () => {
      realtimeSocket = null;
      const delays = [1500, 3000, 5000, 10000];
      const delay = delays[Math.min(wsReconnectAttempts++, delays.length - 1)];
      setSystemStatusPill('error', '服务重连中...');
      wsReconnectTimer = setTimeout(initRealtimeSocket, delay);
    };
  } catch {
    setSystemStatusPill('warn', 'HTTP 轮询模式');
    wsReconnectTimer = setTimeout(initRealtimeSocket, 5000);
  }
}

// ======================== 数据拉取与渲染 ========================
async function loadConfig() {
  try {
    const res = await fetch('/api/config');
    if (res.ok) {
      systemConfig = await res.json();
      projects = systemConfig.projects || [];
      if (!currentProject && projects.length > 0) {
        currentProject = projects[0];
      } else if (currentProject) {
        currentProject = projects.find(p => p.id === currentProject.id) || projects[0] || null;
      }
      renderProjects();
      renderGroupTabs();
      updateGroupQuickSelectButton();
    }
  } catch (e) {
    console.error('loadConfig error:', e);
  }
}

async function loadDevices() {
  try {
    const res = await fetch('/api/devices');
    if (res.ok) {
      const list = await res.json();
      devices.clear();
      list.forEach(d => devices.set(d.sn, d));
      renderGroupTabs();
      renderMatrix();
      updateTopStats();
      updateGroupQuickSelectButton();
      if (!currentMonitorSn && list.length > 0) {
        selectMonitorDevice(list[0].sn);
      } else if (currentMonitorSn && devices.has(currentMonitorSn)) {
        refreshMonitorTelemetry(devices.get(currentMonitorSn));
      }
    }
  } catch (e) {
    console.error('loadDevices error:', e);
  }
}

function renderProjects() {
  const container = document.getElementById('project-cards-container');
  if (!container) return;
  container.innerHTML = '';

  if (projects.length === 0) {
    container.innerHTML = '<div class="panel-sub" style="grid-column: 1 / -1; text-align: center; padding: 1rem;">暂无体验项目，请点击右上角「管理项目与包名」添加</div>';
    return;
  }

  projects.forEach(proj => {
    const isActive = currentProject && currentProject.id === proj.id;
    const wp = matchWallpaperForProject(proj);
    const card = document.createElement('div');
    card.className = `project-card${isActive ? ' project-card--active' : ''}`;
    card.onclick = () => {
      currentProject = proj;
      playSound('click');
      setWallpaperByUrl(wp.url);
      renderProjects();
    };
    card.innerHTML = `
      <div class="project-card__bg" style="background-image: url('${wp.url}')"></div>
      <div class="project-card__scrim"></div>
      <div class="project-card__top">
        <span class="project-card__icon">${escapeHtml(proj.icon || '🎮')}</span>
        <div class="project-card__meta">
          <span class="status-pill">${proj.type === 'vr_video' ? '视频' : '应用'} · ${escapeHtml(proj.durationMinutes || 15)}m</span>
          <button type="button" class="project-card__edit" title="编辑项目" data-action="openConfigAndEditProject" data-arg="${escapeHtml(proj.id)}" data-stop="1">✏️</button>
        </div>
      </div>
      <div class="project-card__title" title="${escapeHtml(proj.name)}">${escapeHtml(proj.name)}</div>
      <div class="project-card__pkg" title="${escapeHtml(proj.packageName)}">${escapeHtml(proj.packageName)}</div>
    `;
    container.appendChild(card);
  });
}

function isDeviceInGroup(dev, group) {
  if (!group || !Array.isArray(group.deviceIdentifiers)) return false;
  return group.deviceIdentifiers.some(id =>
    id === dev.sn || id === dev.alias || id === dev.suiteNo || id === dev.displayName
  );
}

function getFilteredDevices() {
  const all = Array.from(devices.values()).sort((a, b) => {
    const numA = parseInt((a.alias || '').replace(/\D/g, ''), 10) || 9999;
    const numB = parseInt((b.alias || '').replace(/\D/g, ''), 10) || 9999;
    if (numA !== numB) return numA - numB;
    return (a.displayName || a.sn).localeCompare(b.displayName || b.sn);
  });
  if (currentActiveGroupId === 'all' || !systemConfig?.groups) return all;
  const grp = systemConfig.groups.find(g => g.id === currentActiveGroupId);
  if (!grp) return all;
  return all.filter(d => isDeviceInGroup(d, grp));
}

function renderGroupTabs() {
  const container = document.getElementById('device-group-tabs');
  if (!container) return;
  container.innerHTML = '';

  const allBtn = document.createElement('button');
  allBtn.type = 'button';
  allBtn.className = `group-tab${currentActiveGroupId === 'all' ? ' group-tab--active' : ''}`;
  allBtn.onclick = () => switchGroupTab('all');
  allBtn.innerHTML = `<span>🌐 全部设备</span><span class="group-tab__count">${devices.size}</span>`;
  container.appendChild(allBtn);

  const groups = systemConfig?.groups || [];
  groups.forEach(g => {
    const count = Array.from(devices.values()).filter(d => isDeviceInGroup(d, g)).length;
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `group-tab${currentActiveGroupId === g.id ? ' group-tab--active' : ''}`;
    btn.onclick = () => switchGroupTab(g.id);
    btn.innerHTML = `<span>🏷️ ${escapeHtml(g.name)}</span><span class="group-tab__count">${count}</span>`;
    container.appendChild(btn);
  });
}

function switchGroupTab(groupId) {
  currentActiveGroupId = groupId;
  playSound('click');
  if (groupId !== 'all') {
    const grp = (systemConfig?.groups || []).find(g => g.id === groupId);
    if (grp?.defaultProjectId) {
      const p = projects.find(x => x.id === grp.defaultProjectId || x.packageName === grp.defaultProjectId);
      if (p) {
        currentProject = p;
        renderProjects();
      }
    }
  }
  renderGroupTabs();
  renderMatrix();
  updateGroupQuickSelectButton();
}

function updateGroupQuickSelectButton() {
  const btn = document.getElementById('btn-select-group');
  if (!btn) return;
  if (currentActiveGroupId === 'all') {
    btn.classList.add('hidden');
  } else {
    btn.classList.remove('hidden');
    const onlineInGroup = getFilteredDevices().filter(d => d.online).length;
    safeSetText('btn-total-group', onlineInGroup);
  }
}

function renderMatrix() {
  const matrix = document.getElementById('device-matrix');
  if (!matrix) return;
  matrix.innerHTML = '';

  const list = getFilteredDevices();
  if (list.length === 0) {
    matrix.innerHTML = `
      <div class="panel-sub" style="grid-column: 1 / -1; text-align: center; padding: 2.2rem 1rem;">
        暂无设备。请确认 PICO Business Suite 已登录且头显接入同一局域网，或点击右上角「配置中心」绑定头显 SN。
      </div>`;
    safeSetText('selected-badge', selectedSns.size);
    return;
  }

  list.forEach(dev => {
    const isSelected = selectedSns.has(dev.sn);
    const isMonitoring = currentMonitorSn === dev.sn;
    const isRunning = dev.online && !isDesktopApp(dev.currentApp);
    const isLow = dev.online && dev.battery > 0 && dev.battery < 30;

    let badgeClass = 'status-pill';
    let badgeText = '离线';
    if (dev.online) {
      if (dev.trackingLost) {
        badgeClass = 'status-pill status-pill--error';
        badgeText = '定位丢失';
      } else if (isRunning) {
        badgeClass = 'status-pill status-pill--info';
        badgeText = '体验中';
      } else {
        badgeClass = 'status-pill status-pill--online';
        badgeText = '空闲就绪';
      }
    }

    const batPct = dev.online ? Math.min(100, Math.max(0, dev.battery || 0)) : 0;
    const batClass = isLow ? 'battery-fill--low' : (batPct < 55 ? 'battery-fill--mid' : '');

    const card = document.createElement('div');
    card.className = [
      'device-card',
      isSelected ? 'device-card--selected' : '',
      isMonitoring ? 'device-card--monitoring' : '',
      !dev.online ? 'device-card--offline' : ''
    ].filter(Boolean).join(' ');
    card.dataset.action = 'toggleDeviceSelect';
    card.dataset.arg = dev.sn;

    card.innerHTML = `
      <div class="device-card__head">
        <div class="device-card__id">
          <input type="checkbox" ${isSelected ? 'checked' : ''} aria-label="选择设备 ${escapeHtml(dev.displayName)}">
          <span class="device-card__name" title="${escapeHtml(dev.displayName)}">${escapeHtml(dev.displayName)}</span>
        </div>
        <span class="${badgeClass}"><span class="dot"></span>${badgeText}</span>
      </div>

      <div class="device-card__sn">SN: ${escapeHtml(dev.sn.slice(-8))} ${dev.suiteNo ? `· 套件 #${escapeHtml(dev.suiteNo)}` : ''}</div>

      <div class="device-card__app" title="${escapeHtml(dev.appTitle || dev.currentApp)}">
        ${dev.trackingLost ? '⚠️ 大空间定位丢失' : (isRunning ? `🎮 ${escapeHtml(dev.appTitle || dev.currentApp)}` : '🏠 系统桌面')}
      </div>

      <div class="device-card__foot">
        <div class="battery-bar">
          <span class="mono ${isLow ? 'text-rose' : 'text-secondary'}">${dev.online ? `${batPct}%` : '--'}</span>
          <div class="battery-track"><div class="battery-fill ${batClass}" style="inline-size: ${batPct}%"></div></div>
        </div>
        <button type="button" class="btn btn--xs" data-action="selectMonitorDevice" data-arg="${escapeHtml(dev.sn)}" data-stop="1" title="在右侧查看实时画面与详情">👁 监看</button>
      </div>
    `;
    matrix.appendChild(card);
  });

  safeSetText('selected-badge', selectedSns.size);
}

function toggleDeviceSelect(sn) {
  if (!sn) return;
  if (selectedSns.has(sn)) selectedSns.delete(sn);
  else selectedSns.add(sn);
  playSound('click');
  renderMatrix();
}

function updateTopStats() {
  const all = Array.from(devices.values());
  let idle = 0, running = 0, offline = 0;
  const lowDevs = [];
  all.forEach(d => {
    if (!d.online) {
      offline++;
    } else {
      if (isDesktopApp(d.currentApp)) idle++;
      else running++;
      if (d.battery > 0 && d.battery < 30) lowDevs.push(d);
    }
  });

  safeSetText('stat-total', all.length);
  safeSetText('stat-idle', idle);
  safeSetText('stat-running', running);
  safeSetText('stat-offline', offline);
  safeSetText('stat-low', lowDevs.length);
  safeSetText('btn-total-online', idle + running);
  safeSetText('btn-total-idle', idle);
  safeSetText('btn-total-running', running);

  const alertBox = document.getElementById('low-battery-alert');
  if (alertBox) {
    if (lowDevs.length > 0) {
      safeSetText('low-battery-names', lowDevs.map(d => `${d.displayName}(${d.battery}%)`).join('、'));
      alertBox.classList.remove('hidden');
    } else {
      alertBox.classList.add('hidden');
    }
  }
}

// ======================== 快捷勾选 ========================
function selectAllOnline() {
  selectedSns.clear();
  Array.from(devices.values()).filter(d => d.online).forEach(d => selectedSns.add(d.sn));
  playSound('click');
  renderMatrix();
}

function selectCurrentGroup() {
  selectedSns.clear();
  getFilteredDevices().filter(d => d.online).forEach(d => selectedSns.add(d.sn));
  playSound('click');
  renderMatrix();
}

function selectAllIdle() {
  selectedSns.clear();
  getFilteredDevices().filter(d => d.online && isDesktopApp(d.currentApp)).forEach(d => selectedSns.add(d.sn));
  playSound('click');
  renderMatrix();
}

function selectAllRunning() {
  selectedSns.clear();
  getFilteredDevices().filter(d => d.online && !isDesktopApp(d.currentApp)).forEach(d => selectedSns.add(d.sn));
  playSound('click');
  renderMatrix();
}

function clearSelection() {
  selectedSns.clear();
  playSound('click');
  renderMatrix();
}

// ======================== 右侧监看与遥测 ========================
function selectMonitorDevice(sn) {
  if (isStreamingActive && currentMonitorSn && currentMonitorSn !== sn) {
    stopLiveStreamInternal(currentMonitorSn);
  }
  currentMonitorSn = sn;
  const dev = devices.get(sn);
  if (dev) refreshMonitorTelemetry(dev);
  renderMatrix();
}

function refreshMonitorTelemetry(dev) {
  safeSetText('monitor-panel-title', dev.displayName || dev.sn);
  safeSetText('panel-sn', dev.sn);
  const badge = document.getElementById('monitor-panel-badge');
  if (badge) {
    if (!dev.online) {
      badge.className = 'status-pill';
      badge.textContent = '离线';
    } else if (dev.trackingLost) {
      badge.className = 'status-pill status-pill--error';
      badge.textContent = '定位丢失';
    } else if (!isDesktopApp(dev.currentApp)) {
      badge.className = 'status-pill status-pill--info';
      badge.textContent = '体验中';
    } else {
      badge.className = 'status-pill status-pill--online';
      badge.textContent = '空闲就绪';
    }
  }
  safeSetText('panel-battery', dev.online ? `${dev.battery}%` : '--');
  safeSetText('panel-charging', dev.online ? (dev.isCharging ? '⚡ 充电中' : '') : '');
  safeSetText('panel-controllers', dev.online ? `左 ${dev.controllerLeftBattery}% · 右 ${dev.controllerRightBattery}%` : '左 -- · 右 --');
  safeSetText('panel-worn', dev.online ? (dev.isWorn ? '👓 佩戴中' : '未佩戴') : '--');
  safeSetText('panel-app', dev.trackingLost ? '⚠️ 定位丢失' : (isDesktopApp(dev.currentApp) ? '系统桌面' : (dev.appTitle || dev.currentApp)));
}

async function copyCurrentSn() {
  if (!currentMonitorSn) return;
  const ok = await copyTextToClipboard(currentMonitorSn);
  showToast(ok ? `已复制 SN: ${currentMonitorSn}` : `复制失败，SN: ${currentMonitorSn}`, ok ? '📋' : '⚠️');
}

function startCanvasSimulation() {
  const canvas = document.getElementById('side-monitor-canvas');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  let t = 0;
  function draw() {
    if (!isStreamingActive) {
      ctx.fillStyle = '#070b14';
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.strokeStyle = 'rgba(59, 130, 246, 0.08)';
      ctx.lineWidth = 1;
      for (let x = 0; x < canvas.width; x += 40) {
        ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, canvas.height); ctx.stroke();
      }
      for (let y = 0; y < canvas.height; y += 40) {
        ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(canvas.width, y); ctx.stroke();
      }
      const yScan = (Math.sin(t) * 0.5 + 0.5) * canvas.height;
      const grad = ctx.createLinearGradient(0, yScan - 20, 0, yScan + 20);
      grad.addColorStop(0, 'rgba(6, 182, 212, 0)');
      grad.addColorStop(0.5, 'rgba(6, 182, 212, 0.16)');
      grad.addColorStop(1, 'rgba(6, 182, 212, 0)');
      ctx.fillStyle = grad;
      ctx.fillRect(0, yScan - 20, canvas.width, 40);
      t += 0.018;
    }
    requestAnimationFrame(draw);
  }
  draw();
}

async function checkSuiteHealthBackground() {
  try {
    const res = await fetch('/api/stream/suite-status');
    if (res.ok) {
      lastKnownSuiteHealth = await res.json();
      updateSuiteLinkIndicator(lastKnownSuiteHealth);
    }
  } catch { }
}

function updateSuiteLinkIndicator(health) {
  const el = document.getElementById('suite-link-indicator');
  if (!el || !health) return;
  if (!health.isProcessRunning) {
    el.textContent = '🔴 PICO 套件未运行';
    el.className = 'panel-sub mono text-rose';
  } else if (!health.isPortOpen) {
    el.textContent = '🟡 套件已运行 · 60061 端口未开放';
    el.className = 'panel-sub mono text-amber';
  } else if (!health.isSdkActive) {
    el.textContent = '🟡 SDK 握手中...';
    el.className = 'panel-sub mono text-amber';
  } else {
    el.textContent = '🟢 官方套件 60061 已就绪';
    el.className = 'panel-sub mono text-emerald';
  }
}

async function toggleLiveStream() {
  if (!currentMonitorSn) return;
  const dev = devices.get(currentMonitorSn);
  if (!dev || !dev.online) {
    showToast('当前头显离线，无法开启实时画面监控', '⚠️');
    return;
  }
  playSound('click');

  const streamImg = document.getElementById('side-monitor-img');
  const overlay = document.getElementById('stream-standby-overlay');
  const liveBar = document.getElementById('stream-live-bar');
  const canvas = document.getElementById('side-monitor-canvas');

  if (!isStreamingActive) {
    isStreamingActive = true;
    overlay.classList.add('hidden');
    liveBar.classList.remove('hidden');
    canvas.classList.add('hidden');
    streamImg.classList.remove('hidden');

    await fetch(`/api/stream/${encodeURIComponent(dev.sn)}/start`, { method: 'POST' }).catch(() => {});
    startStreamStatusPolling(dev.sn);

    streamImg.src = apiUrl(`/api/stream/${encodeURIComponent(dev.sn)}/mjpeg?t=${Date.now()}`);
    streamImg.onerror = () => startSnapshotStream(dev.sn);
    showToast(`已开启 ${dev.displayName} 实时监看`);
  } else {
    stopLiveStreamInternal(dev.sn);
    showToast('已停止画面拉流');
  }
}

function startSnapshotStream(sn) {
  if (snapshotTimer) clearInterval(snapshotTimer);
  const streamImg = document.getElementById('side-monitor-img');
  let inFlight = false;
  snapshotTimer = setInterval(async () => {
    if (!isStreamingActive || currentMonitorSn !== sn) {
      clearInterval(snapshotTimer);
      snapshotTimer = null;
      return;
    }
    if (inFlight) return;
    inFlight = true;
    try {
      const res = await fetch(`/api/stream/${encodeURIComponent(sn)}/frame?t=${Date.now()}`);
      if (res.ok) {
        const blob = await res.blob();
        if (blob.size > 0) {
          const url = URL.createObjectURL(blob);
          const old = streamImg.src;
          streamImg.onload = () => { if (old && old.startsWith('blob:')) URL.revokeObjectURL(old); };
          streamImg.src = url;
        }
      }
    } catch { } finally {
      inFlight = false;
    }
  }, 80);
}

function startStreamStatusPolling(sn) {
  stopStreamStatusPolling();
  streamStatusPollTimer = setInterval(async () => {
    if (!isStreamingActive || currentMonitorSn !== sn) {
      stopStreamStatusPolling();
      return;
    }
    try {
      const res = await fetch(`/api/stream/${encodeURIComponent(sn)}/status`);
      if (res.ok) {
        const data = await res.json();
        const st = data.streamStatus;
        if (data.hasRealFrame || st?.phase === 'Streaming') {
          safeSetText('stream-status-text', `LIVE ${st?.lastWidth || 854}x${st?.lastHeight || 480}`);
          safeSetText('monitor-fps-indicator', `${(st?.fps || 0).toFixed(1)} FPS`);
          if (!snapshotTimer) startSnapshotStream(sn);
        } else if (data.suiteHealth && !data.suiteHealth.isPortOpen) {
          safeSetText('stream-status-text', '套件 60061 未就绪');
        } else {
          safeSetText('stream-status-text', '等待头显推流...');
        }
        if (data.suiteHealth) updateSuiteLinkIndicator(data.suiteHealth);
      }
    } catch { }
  }, 1200);
}

function stopStreamStatusPolling() {
  if (streamStatusPollTimer) {
    clearInterval(streamStatusPollTimer);
    streamStatusPollTimer = null;
  }
}

function stopLiveStreamInternal(sn) {
  isStreamingActive = false;
  stopStreamStatusPolling();
  if (snapshotTimer) { clearInterval(snapshotTimer); snapshotTimer = null; }
  const streamImg = document.getElementById('side-monitor-img');
  if (streamImg.src && streamImg.src.startsWith('blob:')) URL.revokeObjectURL(streamImg.src);
  streamImg.src = '';
  streamImg.classList.add('hidden');
  document.getElementById('stream-live-bar')?.classList.add('hidden');
  document.getElementById('stream-standby-overlay')?.classList.remove('hidden');
  document.getElementById('side-monitor-canvas')?.classList.remove('hidden');
  safeSetText('monitor-fps-indicator', '--');
  fetch(`/api/stream/${encodeURIComponent(sn)}/stop`, { method: 'POST' }).catch(() => {});
}

// ======================== 单机与批量控制 ========================
async function singleDeviceAction(action) {
  if (!currentMonitorSn) return showToast('请先选择一台头显', '⚠️');
  const dev = devices.get(currentMonitorSn);
  if (!dev) return;
  playSound('click');

  const sn = encodeURIComponent(dev.sn);
  if (action === 'recenter') {
    await fetch(`/api/devices/${sn}/recenter`, { method: 'POST' });
    showToast(`已向 ${dev.displayName} 下发视角复位`);
  } else if (action === 'home') {
    await fetch(`/api/devices/${sn}/stop-app`, { method: 'POST' });
    showToast(`已退出 ${dev.displayName} 当前应用`);
  } else if (action === 'lock') {
    const endpoint = dev.locked ? 'unlock' : 'lock';
    await fetch(`/api/devices/${sn}/${endpoint}`, { method: 'POST' });
    showToast(`已${dev.locked ? '解锁' : '锁定'} ${dev.displayName}`);
  } else if (action === 'screen_on' || action === 'screen_off') {
    const screenOn = action === 'screen_on';
    await fetch(`/api/devices/${sn}/screen`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ screenOn })
    });
    showToast(`已向 ${dev.displayName} 下发${screenOn ? '亮屏' : '息屏'}`);
  } else if (action === 'find') {
    await fetch(`/api/devices/${sn}/find`, { method: 'POST' });
    showToast(`已向 ${dev.displayName} 发送查找设备提示（亮屏/蜂鸣）`, '🔦');
  } else if (action === 'voice_call') {
    showToast(`语音通话请确保 PICO Business Suite 已登录并连接头显音频通道`, '🎙️');
  } else if (action === 'reboot') {
    if (!confirm(`确认重启头显【${dev.displayName}】吗？`)) return;
    await fetch(`/api/devices/${sn}/reboot`, { method: 'POST' });
    showToast(`已向 ${dev.displayName} 下发重启指令`);
  }
}

async function batchAction(action) {
  const snList = selectedSns.size > 0
    ? Array.from(selectedSns)
    : getFilteredDevices().filter(d => d.online).map(d => d.sn);

  if (snList.length === 0) {
    playSound('warning');
    showToast('请先勾选至少一台在线设备', '⚠️');
    return;
  }

  playSound('click');
  if (action === 'start') {
    if (!currentProject) return showToast('请先在第一步选择一个体验项目', '⚠️');
    const res = await fetch('/api/batch/start-app', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ snList, packageName: currentProject.packageName, extraJson: currentProject.extraJson || '' })
    });
    if (res.ok) {
      playSound('success');
      showToast(`已向 ${snList.length} 台设备启动【${currentProject.name}】`);
    }
  } else if (action === 'pause') {
    await fetch('/api/batch/pause-video', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ snList }) });
    showToast(`已向 ${snList.length} 台设备下发暂停`);
  } else if (action === 'stop') {
    await fetch('/api/batch/stop-app', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ snList }) });
    showToast(`已批量关闭 ${snList.length} 台设备应用`);
  } else if (action === 'screen_on' || action === 'screen_off') {
    const screenOn = action === 'screen_on';
    await fetch('/api/batch/screen', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ snList, screenOn }) });
    showToast(`已对 ${snList.length} 台设备执行批量${screenOn ? '亮屏' : '息屏'}`);
  } else if (action === 'recenter') {
    await fetch('/api/batch/recenter', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ snList }) });
    showToast(`已向 ${snList.length} 台设备批量下发视角复位`, '🎯');
  } else if (action === 'find') {
    await fetch('/api/batch/find', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ snList }) });
    showToast(`已向 ${snList.length} 台设备发送查找定位提示`, '🔦');
  }
}

let globalVolumeDebounce = null;
function onGlobalVolumeInput(val) {
  globalVolume = parseInt(val, 10);
  safeSetText('global-vol-text', `${val}%`);
  if (globalVolumeDebounce) clearTimeout(globalVolumeDebounce);
  globalVolumeDebounce = setTimeout(() => applyGlobalVolume(val, false), 200);
}

async function applyGlobalVolume(val, showFeedback = true) {
  if (globalVolumeDebounce) { clearTimeout(globalVolumeDebounce); globalVolumeDebounce = null; }
  const snList = selectedSns.size > 0
    ? Array.from(selectedSns)
    : Array.from(devices.values()).filter(d => d.online).map(d => d.sn);
  if (snList.length === 0) return;

  await fetch('/api/batch/volume', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ snList, volume: parseInt(val, 10) })
  }).catch(() => {});
  if (showFeedback) showToast(`已同步 ${snList.length} 台设备音量为 ${val}%`, '🔊');
}

function promptBatchSystem(action) {
  const snList = selectedSns.size > 0
    ? Array.from(selectedSns)
    : Array.from(devices.values()).filter(d => d.online).map(d => d.sn);
  if (snList.length === 0) return showToast('当前没有可操作的在线设备', '⚠️');

  playSound('warning');
  pendingSystemAction = action;
  safeSetText('confirm-icon', action === 'reboot' ? '🔄' : '⏻');
  safeSetText('confirm-title', action === 'reboot' ? `确认重启 ${snList.length} 台头显？` : `确认关闭 ${snList.length} 台头显电源？`);
  safeSetText('confirm-desc', action === 'reboot'
    ? '头显将立即重启并在启动后自动重新连接。'
    : '关机后需人工重新按头显电源键开机，请确认现场已结束体验。');
  document.getElementById('confirm-modal')?.classList.remove('hidden');
}

function closeConfirmModal() {
  pendingSystemAction = null;
  document.getElementById('confirm-modal')?.classList.add('hidden');
}

async function executeBatchSystemConfirmed() {
  const action = pendingSystemAction;
  closeConfirmModal();
  if (!action) return;
  const snList = selectedSns.size > 0
    ? Array.from(selectedSns)
    : Array.from(devices.values()).filter(d => d.online).map(d => d.sn);
  await fetch(`/api/batch/${action}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ snList })
  });
  showToast(`已向 ${snList.length} 台设备下发${action === 'reboot' ? '重启' : '关机'}指令`);
}

// ======================== 弹窗：套件诊断 ========================
async function openSuiteDiagModal() {
  document.getElementById('suite-diag-modal')?.classList.remove('hidden');
  safeSetText('diag-target-sn', currentMonitorSn || '未选择');
  safeSetText('diag-time', new Date().toLocaleTimeString());
  await refreshSuiteDiagModal();
}

function closeSuiteDiagModal() {
  document.getElementById('suite-diag-modal')?.classList.add('hidden');
}

async function refreshSuiteDiagModal() {
  try {
    const res = await fetch('/api/stream/suite-status');
    if (!res.ok) return;
    const data = await res.json();
    lastKnownSuiteHealth = data;
    updateSuiteLinkIndicator(data);

    const setBadge = (id, ok, okText, failText) => {
      const el = document.getElementById(id);
      if (!el) return;
      el.textContent = ok ? okText : failText;
      el.className = `status-pill ${ok ? 'status-pill--online' : 'status-pill--error'}`;
    };

    setBadge('diag-proc-badge', data.isProcessRunning, '✅ 已运行', '❌ 未运行');
    setBadge('diag-port-badge', data.isPortOpen, '✅ 端口开放 (60061)', '❌ 未开放 (需登录套件)');
    setBadge('diag-sdk-badge', data.isSdkActive && data.isServerConnected, '✅ 握手正常', data.isPortOpen ? '🟡 握手中' : '⏳ 等待 60061 开放');

    const streamEl = document.getElementById('diag-stream-badge');
    const st = currentMonitorSn && data.streams ? data.streams[currentMonitorSn] : null;
    if (streamEl) {
      if (st?.phase === 'Streaming') {
        streamEl.textContent = `🟢 推流中 (${st.totalFrames} 帧)`;
        streamEl.className = 'status-pill status-pill--online';
      } else if (st?.phase === 'WaitingFirstFrame') {
        streamEl.textContent = '⏳ 等待首帧';
        streamEl.className = 'status-pill status-pill--warn';
      } else {
        streamEl.textContent = '待命中';
        streamEl.className = 'status-pill';
      }
    }

    safeSetText('diag-summary-icon', data.isPortOpen && data.isSdkActive ? '🎉' : '⚠️');
    safeSetText('diag-summary-title', data.isPortOpen && data.isSdkActive ? '官方套件链路通信正常' : '官方套件链路尚未就绪');
    safeSetText('diag-summary-desc', data.statusSummary || '');
    const advice = document.getElementById('diag-advice-text');
    if (advice) {
      advice.innerHTML = `
        <div><strong>诊断建议：</strong>${escapeHtml(data.advice || '请确认 PICO Business Suite 已登录并开启服务模式')}</div>
        <div>1. 官方套件需保持登录状态才会开放本地 60061 gRPC 端口。</div>
        <div>2. 头显需开机亮屏并与管理电脑处于同一局域网。</div>
      `;
    }
  } catch { }
}

async function reconnectSuiteSdk() {
  showToast('正在重新握手 PICO 官方套件...', '🔄');
  const res = await fetch('/api/stream/reconnect', { method: 'POST' });
  if (res.ok) {
    const data = await res.json();
    showToast(data.success ? 'SDK 重新连接成功' : '重连未就绪，请检查官方套件是否已登录', data.success ? '✓' : '⚠️');
    await refreshSuiteDiagModal();
  }
}

// ======================== 弹窗：配置中心 ========================
function openConfigModal(tab = 'app') {
  document.getElementById('config-modal')?.classList.remove('hidden');
  switchConfigTab(tab);
  renderConfigTables();
}

function closeConfigModal() {
  document.getElementById('config-modal')?.classList.add('hidden');
}

function switchConfigTab(tab) {
  ['app', 'headset', 'udp', 'env'].forEach(t => {
    const btn = document.getElementById(`tab-btn-${t}`);
    const content = document.getElementById(`tab-content-${t}`);
    if (btn) btn.className = `tab-btn${t === tab ? ' tab-btn--active' : ''}`;
    if (content) content.classList.toggle('hidden', t !== tab);
  });
  if (tab === 'env') loadPicoServiceStatus();
}

function renderConfigTables() {
  const appTbody = document.getElementById('app-config-tbody');
  if (appTbody) {
    appTbody.innerHTML = '';
    projects.forEach(p => {
      const tr = document.createElement('tr');
      tr.innerHTML = `
        <td><strong>${escapeHtml(p.icon || '🎮')} ${escapeHtml(p.name)}</strong></td>
        <td><span class="status-pill">${p.type === 'vr_video' ? 'VR视频' : 'VR应用'}</span></td>
        <td class="mono text-accent">${escapeHtml(p.packageName)}</td>
        <td class="mono">${escapeHtml(p.durationMinutes || 15)}</td>
        <td class="text-right">
          <button type="button" class="btn btn--xs" data-action="openEditProjectForm" data-arg="${escapeHtml(p.id)}">编辑</button>
          <button type="button" class="btn btn--xs btn--danger" data-action="deleteProject" data-arg="${escapeHtml(p.id)}">删除</button>
        </td>
      `;
      appTbody.appendChild(tr);
    });
  }

  const headsetTbody = document.getElementById('headset-config-tbody');
  if (headsetTbody) {
    headsetTbody.innerHTML = '';
    getFilteredDevices().forEach(dev => {
      const tr = document.createElement('tr');
      tr.innerHTML = `
        <td class="mono"><strong>${escapeHtml(dev.sn)}</strong></td>
        <td class="mono">${dev.suiteNo ? escapeHtml(dev.suiteNo) : '--'}</td>
        <td><strong>${escapeHtml(dev.alias || '未设置')}</strong></td>
        <td>${escapeHtml(dev.model)}</td>
        <td><span class="status-pill ${dev.online ? 'status-pill--online' : ''}">${dev.online ? '在线' : '离线'}</span></td>
        <td class="text-right">
          <button type="button" class="btn btn--xs" data-action="openAliasModal" data-arg="${escapeHtml(dev.sn)}" data-arg2="${escapeHtml(dev.alias || '')}">改别名</button>
          <button type="button" class="btn btn--xs btn--danger" data-action="unbindHeadset" data-arg="${escapeHtml(dev.sn)}">解绑</button>
        </td>
      `;
      headsetTbody.appendChild(tr);
    });
  }

  renderUdpDeviceProjects();
}

function renderUdpDeviceProjects() {
  const tbody = document.getElementById('udp-device-project-tbody');
  if (!tbody) return;
  tbody.innerHTML = '';
  const list = Array.from(devices.values());
  if (list.length === 0) {
    tbody.innerHTML = '<tr><td colspan="4" class="panel-sub" style="text-align: center;">暂无设备</td></tr>';
    return;
  }
  const defaults = systemConfig?.deviceDefaultProjects || {};
  list.forEach(dev => {
    const devKey = dev.alias || dev.sn;
    const bound = defaults[dev.alias] || defaults[dev.sn] || '';
    const opts = ['<option value="">-- 默认使用第1个项目 --</option>'].concat(
      projects.map(p => `<option value="${escapeHtml(p.packageName)}" ${(bound === p.packageName || bound === p.id) ? 'selected' : ''}>${escapeHtml(p.icon || '🎮')} ${escapeHtml(p.name)}</option>`)
    ).join('');
    const tr = document.createElement('tr');
    tr.innerHTML = `
      <td><strong>${escapeHtml(dev.displayName)}</strong></td>
      <td class="mono panel-sub">${escapeHtml(dev.sn)}</td>
      <td><select data-change="saveDeviceProject" data-arg="${escapeHtml(devKey)}" style="inline-size: 100%;">${opts}</select></td>
      <td class="text-right"><span class="status-pill ${dev.online ? 'status-pill--online' : ''}">${dev.online ? '在线' : '离线'}</span></td>
    `;
    tbody.appendChild(tr);
  });
}

async function saveDeviceProject(deviceKey, packageOrProjectId) {
  const res = await fetch('/api/config/device-project', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ deviceKey, packageOrProjectId })
  });
  if (res.ok) {
    showToast(`已保存【${deviceKey}】默认绑定项目`);
    await loadConfig();
  }
}

function updateTestUdpPreview() {
  const devNo = document.getElementById('test-udp-devno')?.value?.trim() || '022';
  const action = document.getElementById('test-udp-action')?.value || 'start';
  const pkg = document.getElementById('test-udp-pkg')?.value?.trim();
  const payload = { deviceNo: devNo, action };
  if (pkg) payload.packageName = pkg;
  safeSetText('test-udp-preview-json', `报文预览: ${JSON.stringify(payload)}`);
}

function onTestUdpActionChange(action) {
  const input = document.getElementById('test-udp-pkg');
  if (input) input.disabled = action !== 'start';
  updateTestUdpPreview();
}

async function sendTestUdpMessage() {
  const deviceNo = document.getElementById('test-udp-devno')?.value?.trim() || '022';
  const action = document.getElementById('test-udp-action')?.value || 'start';
  const packageName = document.getElementById('test-udp-pkg')?.value?.trim() || '';
  const box = document.getElementById('test-udp-result-box');
  if (box) {
    box.classList.remove('hidden');
    box.textContent = '正在发送 UDP 报文...';
  }
  const res = await fetch('/api/config/test-udp', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ deviceNo, action, packageName })
  });
  const data = await res.json();
  if (box) box.textContent = typeof data.ack === 'string' ? data.ack : JSON.stringify(data, null, 2);
  showToast(data.success ? 'UDP 测试指令已回执' : 'UDP 测试失败', data.success ? '📡' : '⚠️');
}

function openAddProjectForm() {
  document.getElementById('project-form-container')?.classList.remove('hidden');
  safeSetText('project-form-title', '添加新体验项目');
  document.getElementById('form-proj-id').value = '';
  document.getElementById('form-proj-name').value = '';
  document.getElementById('form-proj-package').value = '';
  document.getElementById('form-proj-type').value = 'vr_app';
  document.getElementById('form-proj-duration').value = '15';
  document.getElementById('form-proj-icon').value = '🎮';
}

function openEditProjectForm(id) {
  const p = projects.find(x => x.id === id);
  if (!p) return;
  document.getElementById('project-form-container')?.classList.remove('hidden');
  safeSetText('project-form-title', `编辑项目：${p.name}`);
  document.getElementById('form-proj-id').value = p.id;
  document.getElementById('form-proj-name').value = p.name;
  document.getElementById('form-proj-package').value = p.packageName;
  document.getElementById('form-proj-type').value = p.type || 'vr_app';
  document.getElementById('form-proj-duration').value = p.durationMinutes || 15;
  document.getElementById('form-proj-icon').value = p.icon || '🎮';
}

function closeProjectForm() {
  document.getElementById('project-form-container')?.classList.add('hidden');
}

async function submitProjectForm() {
  const id = document.getElementById('form-proj-id').value.trim();
  const name = document.getElementById('form-proj-name').value.trim();
  const packageName = document.getElementById('form-proj-package').value.trim();
  const type = document.getElementById('form-proj-type').value;
  const durationMinutes = parseInt(document.getElementById('form-proj-duration').value, 10) || 15;
  const icon = document.getElementById('form-proj-icon').value.trim() || '🎮';
  if (!name || !packageName) return showToast('请填写项目名称和包名', '⚠️');

  const res = await fetch('/api/config/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: id || '', name, packageName, type, durationMinutes, icon })
  });
  if (res.ok) {
    closeProjectForm();
    await loadConfig();
    renderConfigTables();
    showToast(`项目【${name}】已保存`);
  }
}

async function deleteProject(id) {
  if (!confirm('确认删除此体验项目吗？')) return;
  const res = await fetch(`/api/config/projects/${encodeURIComponent(id)}`, { method: 'DELETE' });
  if (res.ok) {
    await loadConfig();
    renderConfigTables();
    showToast('项目已删除');
  }
}

function openBindHeadsetForm() {
  document.getElementById('headset-bind-form-container')?.classList.remove('hidden');
  document.getElementById('form-bind-sn').value = '';
  document.getElementById('form-bind-alias').value = '';
}

function closeBindHeadsetForm() {
  document.getElementById('headset-bind-form-container')?.classList.add('hidden');
}

async function submitBindHeadsetForm() {
  const sn = document.getElementById('form-bind-sn').value.trim().toUpperCase();
  const alias = document.getElementById('form-bind-alias').value.trim();
  const model = document.getElementById('form-bind-model').value;
  if (!sn) return showToast('请填写头显 SN 序列号', '⚠️');
  const res = await fetch('/api/config/bind-headset', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ sn, alias, model })
  });
  if (res.ok) {
    closeBindHeadsetForm();
    await loadDevices();
    renderConfigTables();
    showToast(`已绑定头显 ${sn}`);
  }
}

async function unbindHeadset(sn) {
  if (!confirm(`确认解绑头显【${sn}】吗？`)) return;
  const res = await fetch(`/api/config/bind-headset/${encodeURIComponent(sn)}`, { method: 'DELETE' });
  if (res.ok) {
    devices.delete(sn);
    selectedSns.delete(sn);
    await loadDevices();
    renderConfigTables();
    showToast(`已解绑头显 ${sn}`);
  }
}

// ======================== PICO 服务模式 ========================
async function loadPicoServiceStatus() {
  try {
    const res = await fetch('/api/config/pico-service-mode');
    if (!res.ok) return;
    const data = await res.json();
    safeSetText('pico-ini-path', data.iniPath || '未检测到');
    const badge = document.getElementById('pico-service-badge');
    if (badge) {
      badge.textContent = data.runAsService ? '🟢 已开启服务模式' : '🔴 未开启服务模式';
      badge.className = `status-pill ${data.runAsService ? 'status-pill--online' : 'status-pill--error'}`;
    }
    const proc = document.getElementById('pico-proc-status');
    if (proc) {
      proc.textContent = data.isProcessRunning ? `🟢 运行中 (PID ${data.processId})` : '⚪ 未运行';
    }
  } catch { }
}

async function setPicoServiceMode(enable) {
  const res = await fetch('/api/config/pico-service-mode', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enable, restartProcess: false })
  });
  const data = await res.json();
  showToast(data.message || '操作完成', data.success ? '✓' : '⚠️');
  loadPicoServiceStatus();
}

async function restartPicoService() {
  showToast('正在重启 PICO 官方套件...', '🔄');
  const res = await fetch('/api/config/pico-service-mode/restart', { method: 'POST' });
  const data = await res.json();
  showToast(data.message || '已触发重启', data.success ? '✓' : '⚠️');
  setTimeout(loadPicoServiceStatus, 1500);
}

// ======================== 弹窗：别名修改 ========================
function openCurrentAliasModal() {
  if (!currentMonitorSn) return showToast('请先选择一台头显', '⚠️');
  const dev = devices.get(currentMonitorSn);
  openAliasModal(currentMonitorSn, dev?.alias || '');
}

function openAliasModal(sn, alias) {
  document.getElementById('alias-modal-sn').value = sn;
  document.getElementById('alias-modal-input').value = alias || '';
  document.getElementById('alias-modal')?.classList.remove('hidden');
  document.getElementById('alias-modal-input')?.focus();
}

function closeAliasModal() {
  document.getElementById('alias-modal')?.classList.add('hidden');
}

async function submitAliasModal() {
  const sn = document.getElementById('alias-modal-sn').value.trim();
  const alias = document.getElementById('alias-modal-input').value.trim();
  const res = await fetch('/api/config/alias', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ sn, alias })
  });
  if (res.ok) {
    closeAliasModal();
    await loadDevices();
    renderConfigTables();
    showToast(`已更新 ${sn} 别名为 ${alias || sn}`);
  }
}

// ======================== 弹窗：设备分组管理 ========================
function openGroupManageModal() {
  document.getElementById('group-modal')?.classList.remove('hidden');
  renderGroupManageList();
  cancelGroupForm();
}

function closeGroupManageModal() {
  document.getElementById('group-modal')?.classList.add('hidden');
}

function renderGroupManageList() {
  const container = document.getElementById('group-list-container');
  if (!container) return;
  container.innerHTML = '';
  const groups = systemConfig?.groups || [];
  if (groups.length === 0) {
    container.innerHTML = '<div class="panel-sub" style="text-align: center; padding: 1rem;">暂无分组，点击「新建分组」创建区域分组</div>';
    return;
  }
  groups.forEach(g => {
    const row = document.createElement('div');
    row.className = 'diag-row';
    row.innerHTML = `
      <div>
        <strong>🏷️ ${escapeHtml(g.name)}</strong>
        <span class="panel-sub" style="margin-inline-start: 0.5rem;">包含 ${(g.deviceIdentifiers || []).length} 台设备</span>
      </div>
      <div class="flex-row">
        <button type="button" class="btn btn--xs" data-action="editGroup" data-arg="${escapeHtml(g.id)}">编辑</button>
        <button type="button" class="btn btn--xs btn--danger" data-action="deleteGroup" data-arg="${escapeHtml(g.id)}">删除</button>
      </div>
    `;
    container.appendChild(row);
  });
}

function populateGroupFormSelectors(selectedIds = [], selectedProj = '') {
  const sel = document.getElementById('group-form-project');
  if (sel) {
    sel.innerHTML = '<option value="">-- 不预设 --</option>' +
      projects.map(p => `<option value="${escapeHtml(p.id)}" ${p.id === selectedProj ? 'selected' : ''}>${escapeHtml(p.icon || '🎮')} ${escapeHtml(p.name)}</option>`).join('');
  }
  const grid = document.getElementById('group-device-selector-grid');
  if (grid) {
    grid.innerHTML = '';
    const all = Array.from(devices.values());
    if (all.length === 0) {
      grid.innerHTML = '<span class="panel-sub">暂无已知头显</span>';
      return;
    }
    all.forEach(d => {
      const checked = selectedIds.includes(d.sn) || selectedIds.includes(d.alias);
      const lbl = document.createElement('label');
      lbl.className = 'flex-row';
      lbl.style.fontSize = '0.78rem';
      lbl.style.cursor = 'pointer';
      lbl.innerHTML = `<input type="checkbox" name="group-dev-cb" value="${escapeHtml(d.sn)}" ${checked ? 'checked' : ''}> <span>${escapeHtml(d.displayName)}</span>`;
      grid.appendChild(lbl);
    });
  }
}

function openNewGroupForm() {
  document.getElementById('group-edit-form')?.classList.remove('hidden');
  safeSetText('group-form-title', '新建设备分组');
  document.getElementById('group-form-id').value = '';
  document.getElementById('group-form-name').value = '';
  populateGroupFormSelectors([], '');
}

function editGroup(id) {
  const g = (systemConfig?.groups || []).find(x => x.id === id);
  if (!g) return;
  document.getElementById('group-edit-form')?.classList.remove('hidden');
  safeSetText('group-form-title', `编辑分组：${g.name}`);
  document.getElementById('group-form-id').value = g.id;
  document.getElementById('group-form-name').value = g.name;
  populateGroupFormSelectors(g.deviceIdentifiers || [], g.defaultProjectId || '');
}

function cancelGroupForm() {
  document.getElementById('group-edit-form')?.classList.add('hidden');
}

function toggleAllGroupDevices(checked) {
  document.querySelectorAll('input[name="group-dev-cb"]').forEach(cb => { cb.checked = checked; });
}

async function saveGroupFromForm() {
  const id = document.getElementById('group-form-id').value.trim();
  const name = document.getElementById('group-form-name').value.trim();
  const defaultProjectId = document.getElementById('group-form-project').value;
  const deviceIdentifiers = Array.from(document.querySelectorAll('input[name="group-dev-cb"]:checked')).map(cb => cb.value);
  if (!name) return showToast('请输入分组名称', '⚠️');

  const res = await fetch('/api/config/groups', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: id || '', name, defaultProjectId, deviceIdentifiers })
  });
  if (res.ok) {
    cancelGroupForm();
    await loadConfig();
    renderGroupManageList();
    showToast(`分组【${name}】已保存`);
  }
}

async function deleteGroup(id) {
  if (!confirm('确认删除该分组吗？')) return;
  const res = await fetch(`/api/config/groups/${encodeURIComponent(id)}`, { method: 'DELETE' });
  if (res.ok) {
    if (currentActiveGroupId === id) currentActiveGroupId = 'all';
    await loadConfig();
    renderGroupManageList();
    showToast('分组已删除');
  }
}

// ======================== 弹窗：无线 ADB 与批量部署 ========================
function openAdbModal() {
  document.getElementById('adb-modal')?.classList.remove('hidden');
  loadAdbDevices();
}

function closeAdbModal() {
  document.getElementById('adb-modal')?.classList.add('hidden');
}

function switchAdbTab(tab) {
  const isDev = tab === 'devices';
  document.getElementById('adb-tab-btn-devices').className = `tab-btn${isDev ? ' tab-btn--active' : ''}`;
  document.getElementById('adb-tab-btn-deploy').className = `tab-btn${!isDev ? ' tab-btn--active' : ''}`;
  document.getElementById('adb-tab-content-devices').classList.toggle('hidden', !isDev);
  document.getElementById('adb-tab-content-deploy').classList.toggle('hidden', isDev);
  if (!isDev) renderDeployTargetCheckboxes();
}

async function loadAdbDevices() {
  try {
    const res = await fetch('/api/adb/devices');
    if (res.ok) {
      const data = await res.json();
      adbDevicesList = data.devices || [];
      renderAdbTable();
      renderDeployTargetCheckboxes();
    }
  } catch { }
}

function renderAdbTable() {
  const tbody = document.getElementById('adb-devices-tbody');
  if (!tbody) return;
  const connected = adbDevicesList.filter(d => d.isConnected).length;
  safeSetText('adb-devices-count-badge', `${connected} 台已连接 (共 ${adbDevicesList.length} 台)`);
  tbody.innerHTML = '';

  if (adbDevicesList.length === 0) {
    tbody.innerHTML = '<tr><td colspan="7" class="panel-sub" style="text-align: center; padding: 1.5rem;">暂无已连接的 ADB 设备，请点击「全网段自动扫描」或手动输入 IP 连接</td></tr>';
    return;
  }

  adbDevicesList.forEach(dev => {
    const tr = document.createElement('tr');
    const canSync = dev.isConnected && !!dev.sn;
    tr.innerHTML = `
      <td><input type="checkbox" name="adb-sync-target" value="${escapeHtml(dev.target)}" ${canSync ? 'checked' : 'disabled'}></td>
      <td class="mono"><strong>${dev.isWireless ? '📶' : '🔌'} ${escapeHtml(dev.target)}</strong></td>
      <td class="mono text-accent">${dev.sn ? escapeHtml(dev.sn) : '--'}</td>
      <td>${escapeHtml(dev.model || 'PICO VR')}</td>
      <td class="mono">${dev.battery >= 0 ? `${dev.battery}%` : '--'}</td>
      <td><span class="status-pill ${dev.isConnected ? 'status-pill--online' : ''}">${dev.isConnected ? '已就绪' : escapeHtml(dev.status)}</span></td>
      <td class="text-right">
        <button type="button" class="btn btn--xs" data-action="readAdbSn" data-arg="${escapeHtml(dev.target)}">读SN</button>
        <button type="button" class="btn btn--xs btn--amber" data-action="rebootAdb" data-arg="${escapeHtml(dev.target)}">重启</button>
        <button type="button" class="btn btn--xs btn--danger" data-action="disconnectAdb" data-arg="${escapeHtml(dev.target)}">断开</button>
      </td>
    `;
    tbody.appendChild(tr);
  });
}

function toggleAllAdbSyncSelect(checked) {
  document.querySelectorAll('input[name="adb-sync-target"]:not(:disabled)').forEach(cb => {
    cb.checked = !!checked;
  });
}

async function startAdbScan() {
  const btn = document.getElementById('adb-scan-btn');
  const prog = document.getElementById('adb-scan-progress');
  if (btn) btn.disabled = true;
  if (prog) prog.classList.remove('hidden');
  safeSetText('adb-scan-text', '扫描探测中...');
  try {
    const res = await fetch('/api/adb/scan', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' });
    if (res.ok) {
      const data = await res.json();
      adbDevicesList = data.devices || [];
      renderAdbTable();
      renderDeployTargetCheckboxes();
      showToast(`扫描完成，发现 ${data.discoveredCount || 0} 台设备`);
    }
  } finally {
    if (btn) btn.disabled = false;
    if (prog) prog.classList.add('hidden');
    safeSetText('adb-scan-text', '全网段自动扫描并连接头显');
  }
}

async function connectManualAdb() {
  const ip = document.getElementById('adb-manual-ip').value.trim();
  const port = parseInt(document.getElementById('adb-manual-port').value, 10) || 5555;
  if (!ip) return showToast('请输入设备 IP', '⚠️');
  const res = await fetch('/api/adb/connect', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ip, port })
  });
  const data = await res.json();
  showToast(data.message || (data.success ? '连接成功' : '连接失败'), data.success ? '✓' : '⚠️');
  if (data.success) {
    document.getElementById('adb-manual-ip').value = '';
    await loadAdbDevices();
  }
}

async function disconnectAdb(target) {
  await fetch('/api/adb/disconnect', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ target }) });
  showToast(`已断开 ${target}`);
  await loadAdbDevices();
}

async function readAdbSn(target) {
  const res = await fetch('/api/adb/read-sn', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ target }) });
  const data = await res.json();
  if (data.success && data.sn) {
    const copied = await copyTextToClipboard(data.sn);
    showToast(copied ? `已读取并复制 SN: ${data.sn}` : `读取到 SN: ${data.sn}`, copied ? '📋' : '✓');
  } else {
    showToast('读取 SN 失败', '⚠️');
  }
  await loadAdbDevices();
}

async function rebootAdb(target) {
  if (!confirm(`确认重启 ${target} 吗？`)) return;
  await fetch('/api/adb/reboot', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ target }) });
  showToast(`已向 ${target} 发送重启指令`);
}

async function syncAdbDevicesToConfig() {
  const checked = Array.from(document.querySelectorAll('input[name="adb-sync-target"]:checked')).map(cb => cb.value);
  if (checked.length === 0) {
    showToast('请先在下方列表中勾选需要同步的设备', '⚠️');
    return;
  }
  const res = await fetch('/api/adb/sync-to-config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targets: checked })
  });
  const data = await res.json();
  showToast(`已将所选 ${checked.length} 台中的 ${data.syncedCount || 0} 台新设备同步至机号列表`);
  await loadDevices();
}

function renderDeployTargetCheckboxes() {
  const container = document.getElementById('deploy-devices-checkboxes');
  if (!container) return;
  const connected = adbDevicesList.filter(d => d && d.isConnected);
  container.innerHTML = '';
  if (connected.length === 0) {
    container.innerHTML = '<span class="panel-sub">暂无已连接 ADB 设备，请先扫描或手动连接</span>';
    safeSetText('deploy-selected-count', '0 台可用');
    return;
  }
  connected.forEach(dev => {
    const lbl = document.createElement('label');
    lbl.className = 'btn btn--xs';
    lbl.innerHTML = `<input type="checkbox" name="deploy-device-target" value="${escapeHtml(dev.target)}" checked data-change="updateDeploySelectedCount"> <span class="mono">${escapeHtml(dev.target)}</span>`;
    container.appendChild(lbl);
  });
  updateDeploySelectedCount();
}

function updateDeploySelectedCount() {
  const count = document.querySelectorAll('input[name="deploy-device-target"]:checked').length;
  safeSetText('deploy-selected-count', `已选 ${count} 台`);
}

function toggleSelectAllDeployDevices(select) {
  document.querySelectorAll('input[name="deploy-device-target"]').forEach(cb => { cb.checked = select; });
  updateDeploySelectedCount();
}

function getSelectedDeployTargets() {
  return Array.from(document.querySelectorAll('input[name="deploy-device-target"]:checked')).map(cb => cb.value);
}

function appendDeployLog(msg, isErr = false) {
  const box = document.getElementById('deploy-console-box');
  const logs = document.getElementById('deploy-console-logs');
  if (!box || !logs) return;
  box.classList.remove('hidden');
  const line = document.createElement('div');
  line.style.color = isErr ? '#fb7185' : '#34d399';
  line.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
  logs.appendChild(line);
  deployLogLines++;
  safeSetText('deploy-log-counter', `${deployLogLines} 条`);
  logs.scrollTop = logs.scrollHeight;
}

function clearDeployLog() {
  const logs = document.getElementById('deploy-console-logs');
  if (logs) logs.innerHTML = '';
  deployLogLines = 0;
  safeSetText('deploy-log-counter', '0 条');
}

async function copyDeployLog() {
  const logs = document.getElementById('deploy-console-logs');
  if (!logs || !logs.textContent.trim()) return;
  const ok = await copyTextToClipboard(logs.textContent);
  showToast(ok ? '已复制部署日志' : '复制失败', ok ? '📋' : '⚠️');
}

function toggleDeployLogHeight() {
  const logs = document.getElementById('deploy-console-logs');
  if (!logs) return;
  isDeployLogExpanded = !isDeployLogExpanded;
  logs.classList.toggle('console-box--expanded', isDeployLogExpanded);
  safeSetText('btn-deploy-log-toggle', isDeployLogExpanded ? '⛶ 还原' : '⛶ 展开');
}

async function executeDeployInstallApk() {
  const localPath = document.getElementById('deploy-apk-localpath').value.trim();
  const file = document.getElementById('deploy-apk-file').files?.[0];
  if (!localPath && !file) return showToast('请提供本地 APK 路径或选择文件', '⚠️');
  const targets = getSelectedDeployTargets();
  if (targets.length === 0) return showToast('请至少勾选一台目标设备', '⚠️');

  if (localPath) {
    appendDeployLog(`开始本地直推安装 APK: ${localPath} -> ${targets.join(', ')}`);
    const res = await fetch('/api/adb/local-install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ localPath, targets })
    });
    const data = await res.json();
    (data.results || []).forEach(r => appendDeployLog(`[${r.target}] ${r.message}`, !r.success));
    if (!data.success) appendDeployLog(data.message || '安装失败', true);
    return;
  }

  appendDeployLog(`开始上传并安装 APK: ${file.name}`);
  const fd = new FormData();
  fd.append('file', file);
  fd.append('targets', targets.join(','));
  const res = await fetch('/api/adb/upload-install', { method: 'POST', body: fd });
  const data = await res.json();
  (data.results || []).forEach(r => appendDeployLog(`[${r.target}] ${r.message}`, !r.success));
  if (!data.success) appendDeployLog(data.message || '安装失败', true);
}

async function executeDeployPushFile() {
  const localPath = document.getElementById('deploy-push-localpath').value.trim();
  const file = document.getElementById('deploy-push-file').files?.[0];
  if (!localPath && !file) return showToast('请提供本地路径或选择上传文件', '⚠️');
  const targets = getSelectedDeployTargets();
  if (targets.length === 0) return showToast('请至少勾选一台目标设备', '⚠️');
  const remotePath = document.getElementById('deploy-push-remote').value.trim() || '/sdcard/Download/';
  const overwrite = document.getElementById('deploy-push-overwrite')?.checked ?? true;
  const flatten = document.getElementById('deploy-push-flatten')?.checked ?? false;

  if (localPath) {
    appendDeployLog(`开始本地推送: ${localPath} -> ${remotePath}`);
    const res = await fetch('/api/adb/local-push', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ localPath, remotePath, targets, overwrite, flatten })
    });
    const data = await res.json();
    (data.results || []).forEach(r => appendDeployLog(`[${r.target}] ${r.message}`, !r.success));
    if (!data.success) appendDeployLog(data.message || '推送失败', true);
    return;
  }

  appendDeployLog(`开始上传并推送: ${file.name} -> ${remotePath}`);
  const fd = new FormData();
  fd.append('file', file);
  fd.append('remotePath', remotePath);
  fd.append('targets', targets.join(','));
  fd.append('overwrite', String(overwrite));
  fd.append('flatten', String(flatten));
  const res = await fetch('/api/adb/upload-push', { method: 'POST', body: fd });
  const data = await res.json();
  (data.results || []).forEach(r => appendDeployLog(`[${r.target}] ${r.message}`, !r.success));
  if (!data.success) appendDeployLog(data.message || '推送失败', true);
}

async function executeDeployCreateFolder() {
  const remotePath = document.getElementById('deploy-push-remote').value.trim();
  const targets = getSelectedDeployTargets();
  if (!remotePath || targets.length === 0) return showToast('请填写远程目录并选择设备', '⚠️');
  const res = await fetch('/api/adb/mkdir', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ remotePath, targets })
  });
  const data = await res.json();
  (data.results || []).forEach(r => appendDeployLog(`[${r.target}] ${r.message}`, !r.success));
}

async function queryDeployPackages() {
  const targets = getSelectedDeployTargets();
  if (targets.length === 0) return showToast('请先在上方「目标头显设备」中勾选至少一台设备', '⚠️');
  const target = targets[0];
  const devInfo = adbDevicesList.find(d => d.target === target);
  const label = devInfo?.sn ? `${target} (${devInfo.sn.slice(-6)})` : target;
  safeSetText('deploy-pkg-source-badge', `读取中: ${label}`);
  const res = await fetch(`/api/adb/packages?target=${encodeURIComponent(target)}`);
  const data = await res.json();
  const dd = document.getElementById('deploy-pkg-dropdown');
  if (data.success && data.packages?.length && dd) {
    dd.innerHTML = `<option value="">-- 来自 ${escapeHtml(label)} 的第三方应用 (${data.packages.length} 个) --</option>` +
      data.packages.map(p => `<option value="${escapeHtml(p)}">${escapeHtml(p)}</option>`).join('');
    dd.classList.remove('hidden');
    safeSetText('deploy-pkg-source-badge', `来源: ${label} · ${data.packages.length} 个应用`);
    showToast(`已从 ${label} 读取 ${data.packages.length} 个应用`);
  } else {
    safeSetText('deploy-pkg-source-badge', `来源: ${label} · 未读取到应用`);
    showToast(`未能从 ${label} 读取到第三方应用`, '⚠️');
  }
}

async function executeDeployUninstallApp() {
  const packageName = document.getElementById('deploy-uninstall-pkg').value.trim();
  const targets = getSelectedDeployTargets();
  if (!packageName || targets.length === 0) return showToast('请输入包名并在上方勾选目标设备', '⚠️');
  if (!confirm(`确认从以下 ${targets.length} 台选中头显卸载【${packageName}】吗？\n${targets.join(', ')}`)) return;
  const res = await fetch('/api/adb/uninstall', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ packageName, targets })
  });
  const data = await res.json();
  (data.results || []).forEach(r => appendDeployLog(`[${r.target}] ${r.message}`, !r.success));
}

// ======================== 弹窗：版本信息 ========================
async function openOtaModal() {
  document.getElementById('ota-modal')?.classList.remove('hidden');
  try {
    const res = await fetch('/api/ota/version');
    if (res.ok) {
      const info = await res.json();
      safeSetText('ota-current-ver', info.version || 'v2.0.0');
      safeSetText('ota-build-time', `构建时间: ${info.buildTime || '2026-11-20'} · ${info.channel || 'Rust 原生一体版'}`);
    }
  } catch { }
}

function closeOtaModal() {
  document.getElementById('ota-modal')?.classList.add('hidden');
}
