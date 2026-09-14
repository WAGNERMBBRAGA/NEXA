const { app, BrowserWindow, dialog, ipcMain, safeStorage } = require('electron');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const net = require('net');

let backend;
let mainWindow;
let startupSecrets = { openaiApiKey: '', embeddingsApiKey: '' };
let backendPort;
const apiToken = crypto.randomBytes(32).toString('hex');
const instanceLock = app.requestSingleInstanceLock();

if (!instanceLock) {
  app.quit();
} else {
  app.on('second-instance', () => {
    if (!mainWindow) return;
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.focus();
  });
}

function secretsFile() {
  return path.join(app.getPath('userData'), 'secrets.json');
}

function saveSecrets(values = {}) {
  if (!safeStorage.isEncryptionAvailable()) throw new Error('O armazenamento seguro do Windows não está disponível.');
  const current = loadSecrets();
  const encrypted = {};
  for (const key of ['openaiApiKey', 'embeddingsApiKey']) {
    const value = Object.prototype.hasOwnProperty.call(values, key) ? String(values[key] || '') : String(current[key] || '');
    encrypted[key] = value ? safeStorage.encryptString(value).toString('base64') : '';
    startupSecrets[key] = value;
  }
  fs.mkdirSync(app.getPath('userData'), { recursive: true });
  fs.writeFileSync(secretsFile(), JSON.stringify(encrypted), { encoding: 'utf8', mode: 0o600 });
  return { saved: true };
}

function loadSecrets() {
  try {
    if (!safeStorage.isEncryptionAvailable() || !fs.existsSync(secretsFile())) return { openaiApiKey: '', embeddingsApiKey: '' };
    const encrypted = JSON.parse(fs.readFileSync(secretsFile(), 'utf8'));
    const result = {};
    for (const key of ['openaiApiKey', 'embeddingsApiKey']) {
      result[key] = encrypted[key] ? safeStorage.decryptString(Buffer.from(encrypted[key], 'base64')) : '';
    }
    return result;
  } catch (error) {
    console.error('Falha ao abrir segredos do NEXA:', error.message);
    return { openaiApiKey: '', embeddingsApiKey: '' };
  }
}

function migratePlaintextSecrets() {
  const configPath = path.join(app.getPath('userData'), 'config.json');
  if (!fs.existsSync(configPath)) return;
  try {
    const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    const openaiApiKey = String(config.openai?.apiKey || '');
    const embeddingsApiKey = String(config.embeddings?.apiKey || '');
    if (!openaiApiKey && !embeddingsApiKey) return;
    saveSecrets({ openaiApiKey, embeddingsApiKey });
    if (config.openai) config.openai.apiKey = '';
    if (config.embeddings) config.embeddings.apiKey = '';
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2), 'utf8');
  } catch (error) {
    console.error('Falha ao migrar segredos antigos:', error.message);
  }
}

function reserveFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(error => error ? reject(error) : resolve(port));
    });
  });
}

function startBackend() {
  const root = app.isPackaged
    ? path.join(process.resourcesPath, 'app.asar.unpacked')
    : path.join(__dirname, '..');
  const backendDir = path.join(root, 'backend');
  const backendLog = path.join(app.getPath('userData'), 'backend.log');
  fs.writeFileSync(backendLog, `NEXA backend starting: ${new Date().toISOString()}\n`);
  backend = spawn(process.execPath, [path.join(backendDir, 'src', 'server.js')], {
    cwd: backendDir,
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
    // No instalador process.execPath é o NEXA.exe (Electron), não node.exe.
    // Este sinal faz a instância filha executar somente o backend Node.
    env: {
      ...process.env,
      ELECTRON_RUN_AS_NODE: '1',
      NEXA_DESKTOP: '1',
      // Estado mutável nunca deve viver dentro de resources/app.asar.unpacked.
      NEXA_USER_DATA: app.getPath('userData'),
      NEXA_API_TOKEN: apiToken,
      NEXA_HOST: '127.0.0.1',
      OPENAI_API_KEY: startupSecrets.openaiApiKey,
      NEXA_EMBEDDINGS_API_KEY: startupSecrets.embeddingsApiKey,
      PORT: String(backendPort),
      NEXA_FRONTEND_DIST: app.isPackaged
        ? path.join(process.resourcesPath, 'app.asar.unpacked', 'frontend', 'dist')
        : path.join(root, 'frontend', 'dist')
    }
  });
  backend.stdout.on('data', chunk => fs.appendFileSync(backendLog, chunk));
  backend.stderr.on('data', chunk => fs.appendFileSync(backendLog, chunk));
  backend.on('error', error => fs.appendFileSync(backendLog, `spawn error: ${error.message}\n`));
  backend.on('exit', (code, signal) => fs.appendFileSync(backendLog, `backend exited: code=${code}, signal=${signal}\n`));
}

async function waitForBackend() {
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      const response = await fetch(`http://127.0.0.1:${backendPort}/api/config`, {
        headers: { 'X-NEXA-Token': apiToken },
        signal: AbortSignal.timeout(500)
      });
      if (response.ok) return true;
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  return false;
}

function openWindow() {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 960,
    minHeight: 640,
    autoHideMenuBar: true,
    icon: path.join(__dirname, 'assets', 'nexa.svg'),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, 'preload.cjs')
    }
  });
  mainWindow.loadURL(`http://127.0.0.1:${backendPort}/#token=` + encodeURIComponent(apiToken));
  mainWindow.on('closed', () => { mainWindow = null; });
}

if (instanceLock) {
  app.whenReady().then(async () => {
    migratePlaintextSecrets();
    startupSecrets = { ...startupSecrets, ...loadSecrets() };
    ipcMain.handle('nexa:save-secrets', (_event, values) => saveSecrets(values));
    ipcMain.handle('nexa:choose-project-directory', async () => {
      const result = await dialog.showOpenDialog(mainWindow, {
        title: 'Abrir projeto nesta conversa',
        properties: ['openDirectory']
      });
      return result.canceled || !result.filePaths[0] ? null : result.filePaths[0];
    });
    backendPort = await reserveFreePort();
    startBackend();
    if (await waitForBackend()) return openWindow();
    dialog.showErrorBox('NEXA não iniciou', 'O serviço interno não ficou disponível. Consulte backend.log na pasta de dados do NEXA.');
    app.quit();
  });
}
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('before-quit', () => { if (backend && !backend.killed) backend.kill(); });
