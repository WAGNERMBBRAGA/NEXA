/**
 * Kimi K3 Service - NEXA
 *
 * Provedor de IA via kimi-k3-in-c (engine C99 para Kimi K3).
 * Engine pura, sem dependências, embedding via FFI.
 *
 * Kimi K3:
 *   - 2.78T parâmetros totais, ~104B ativos por token
 *   - 93 layers (69 KDA + 24 Gated MLA)
 *   - 896 experts, top-16 selecionados
 *   - MXFP4 nativo (4-bit)
 *   - 8.24 GB RAM mínimo (medido)
 *   - 1.56 TB checkpoint
 *
 * Licença engine: Apache 2.0
 * Licença pesos: Moonshot AI (separada)
 */

const { spawn, execSync } = require('child_process');
const path = require('path');
const fs = require('fs');

// ============================================================================
// CONFIG
// ============================================================================

const PROJECT_ROOT = path.join(__dirname, '..', '..', '..');
const K3_ENGINE_DIR = path.join(PROJECT_ROOT, 'kimi-k3-in-c');
const K3_BIN = path.join(K3_ENGINE_DIR, 'bin', process.platform === 'win32' ? 'k3.exe' : 'k3');
const K3_TRUNK_DIR = path.join(PROJECT_ROOT, 'k3trunk');

// Presets de memória
const PRESETS = {
    laptop: { description: 'Mínimo 8 GB RAM', flag: '--preset laptop' },
    server: { description: 'Rápido, 90/93 layers fixadas', flag: '--preset server' },
    workstation: { description: 'Máximo desempenho', flag: '--preset workstation' },
    ultra: { description: 'Proof of life, 1 token', flag: '--preset ultra' }
};

let k3Process = null;

// ============================================================================
// HEALTH CHECK
// ============================================================================

async function isAvailable() {
    try {
        return fs.existsSync(K3_BIN);
    } catch {
        return false;
    }
}

async function isRunning() {
    return k3Process && !k3Process.killed;
}

// ============================================================================
// LIST PRESETS
// ============================================================================

function listPresets() {
    return Object.entries(PRESETS).map(([name, info]) => ({
        name,
        ...info
    }));
}

// ============================================================================
// GET MODEL INFO
// ============================================================================

function getModelInfo(modelDir) {
    const configPath = path.join(modelDir || '', 'config.json');
    try {
        const raw = fs.readFileSync(configPath, 'utf-8');
        const config = JSON.parse(raw);
        return {
            hidden_size: config.hidden_size,
            num_layers: config.num_hidden_layers,
            vocab_size: config.vocab_size,
            num_experts: config.num_experts || 896,
            num_experts_per_tok: config.num_experts_per_tok || 16,
            max_position_embeddings: config.max_position_embeddings
        };
    } catch {
        return null;
    }
}

// ============================================================================
// RUN INFERENCE
// ============================================================================

async function runInference(options = {}) {
    const {
        modelDir,
        trunkDir = K3_TRUNK_DIR,
        prompt = '',
        preset = 'laptop',
        genTokens = 16,
        incremental = true,
        tokenIds = null,
        onToken = null
    } = options;

    if (!modelDir) throw new Error('modelDir é obrigatório (pasta com .safetensors shards)');

    const args = [modelDir];

    if (trunkDir && fs.existsSync(trunkDir)) {
        args.push('--trunk', trunkDir);
    }

    args.push('--preset', preset || 'laptop');

    if (tokenIds) {
        args.push('--ids', Array.isArray(tokenIds) ? tokenIds.join(',') : tokenIds);
    } else if (prompt) {
        args.push('--prompt', prompt);
    }

    args.push('--gen', String(genTokens || 16));

    if (incremental) {
        args.push('--incremental');
    }

    return new Promise((resolve, reject) => {
        const proc = spawn(K3_BIN, args, {
            stdio: ['ignore', 'pipe', 'pipe']
        });

        k3Process = proc;

        let stdout = '';
        let stderr = '';

        proc.stdout.on('data', (data) => {
            const text = data.toString();
            stdout += text;
            if (onToken) {
                const tokens = text.split(/\s+/).filter(Boolean);
                tokens.forEach(t => onToken(t));
            }
        });

        proc.stderr.on('data', (data) => {
            stderr += data.toString();
        });

        proc.on('close', (code) => {
            k3Process = null;
            if (code === 0) {
                resolve({
                    output: stdout.trim(),
                    exitCode: code,
                    stderr: stderr.trim()
                });
            } else {
                reject(new Error(`k3 exit ${code}: ${stderr.trim() || stdout.trim()}`));
            }
        });

        proc.on('error', (err) => {
            k3Process = null;
            reject(err);
        });
    });
}

// ============================================================================
// STOP
// ============================================================================

function stop() {
    if (k3Process) {
        k3Process.kill();
        k3Process = null;
        return { stopped: true };
    }
    return { alreadyStopped: true };
}

// ============================================================================
// PACK TRUNK
// ============================================================================

async function packTrunk(modelDir, trunkDir) {
    const script = path.join(K3_ENGINE_DIR, 'scripts', 'pack-trunk.sh');
    if (!fs.existsSync(script)) {
        throw new Error('pack-trunk.sh não encontrado');
    }

    return new Promise((resolve, reject) => {
        const proc = spawn('bash', [script, modelDir, trunkDir], {
            stdio: ['ignore', 'pipe', 'pipe']
        });

        let stdout = '';
        let stderr = '';

        proc.stdout.on('data', (d) => { stdout += d.toString(); });
        proc.stderr.on('data', (d) => { stderr += d.toString(); });

        proc.on('close', (code) => {
            if (code === 0) {
                resolve({ ok: true, output: stdout.trim() });
            } else {
                reject(new Error(`pack-trunk failed: ${stderr.trim()}`));
            }
        });

        proc.on('error', reject);
    });
}

// ============================================================================
// CHECK MACHINE
// ============================================================================

async function checkMachine(modelDir) {
    const script = path.join(K3_ENGINE_DIR, 'scripts', 'k3-doctor.sh');
    if (!fs.existsSync(script)) {
        return { available: false, reason: 'k3-doctor.sh não encontrado' };
    }

    return new Promise((resolve) => {
        const proc = spawn('bash', [script, modelDir || ''], {
            stdio: ['ignore', 'pipe', 'pipe']
        });

        let stdout = '';
        proc.stdout.on('data', (d) => { stdout += d.toString(); });
        proc.on('close', () => {
            resolve({ available: true, output: stdout.trim() });
        });
        proc.on('error', () => {
            resolve({ available: false, reason: 'Erro ao executar k3-doctor.sh' });
        });
    });
}

// ============================================================================
// EXPORTS
// ============================================================================

module.exports = {
    PRESETS,
    isAvailable,
    isRunning,
    listPresets,
    getModelInfo,
    runInference,
    stop,
    packTrunk,
    checkMachine,
    K3_ENGINE_DIR,
    K3_BIN
};
