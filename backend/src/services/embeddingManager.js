const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const PROJECT_ROOT = path.join(__dirname, '..', '..', '..');
const MODEL_PATH = path.join(PROJECT_ROOT, 'backend', 'models', 'embeddings', 'nomic-embed-text-v1.5.Q4_K_M.gguf');
let processHandle = null;

function findServer() {
    const candidates = [
        path.join(PROJECT_ROOT, 'llama.cpp', 'bin', 'llama-server.exe'),
        path.join(PROJECT_ROOT, 'llama.cpp-build', 'bin', 'llama-server.exe'),
        path.join(PROJECT_ROOT, 'llama.cpp-build', 'source', 'bin', 'Release', 'llama-server.exe')
    ];
    return candidates.find(candidate => fs.existsSync(candidate)) || null;
}

async function healthy() {
    try { return (await fetch('http://127.0.0.1:8081/health', { signal: AbortSignal.timeout(1500) })).ok; } catch { return false; }
}

async function ensureEmbeddingServer() {
    if (await healthy()) return { available: true, reused: true, modelPath: MODEL_PATH };
    const executable = findServer();
    if (!executable) return { available: false, reason: 'llama-server não encontrado.' };
    if (!fs.existsSync(MODEL_PATH)) return { available: false, reason: 'Modelo local de embeddings não instalado.' };
    processHandle = spawn(executable, [
        '-m', MODEL_PATH, '--embedding', '--pooling', 'mean',
        '-c', '2048', '-b', '512', '-ub', '512', '-np', '1',
        '-ngl', '0', '-fit', 'off',
        '--host', '127.0.0.1', '--port', '8081', '--cache-ram', '0', '--no-warmup'
    ], { cwd: path.dirname(executable), windowsHide: true, stdio: 'ignore' });
    for (let attempt = 0; attempt < 60; attempt++) {
        await new Promise(resolve => setTimeout(resolve, 500));
        if (await healthy()) return { available: true, reused: false, modelPath: MODEL_PATH };
        if (processHandle.exitCode !== null) break;
    }
    return { available: false, reason: 'Servidor local de embeddings não iniciou.' };
}

function stopEmbeddingServer() {
    if (processHandle && !processHandle.killed) processHandle.kill();
    processHandle = null;
}

module.exports = { MODEL_PATH, ensureEmbeddingServer, stopEmbeddingServer };
