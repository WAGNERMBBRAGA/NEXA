/**
 * Model Manager - NEXA
 *
 * Escaneia o computador por modelos GGUF disponíveis (todas as IAs que o
 * usuário já tem instaladas em qualquer pasta) e reinicia o llama-server
 * com o modelo escolhido, dando liberdade total ao usuário para navegar
 * e escolher qualquer IA.
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const { spawn, spawnSync } = require('child_process');

// ============================================================================
// PASTAS RAÍZES PARA ESCANEAR POR MODELOS
// ============================================================================

const PROJECT_ROOT = path.join(__dirname, '..', '..', '..');

// Raízes de busca: pastas onde o usuário guarda modelos.
// Inclui o projeto NEXA, a pasta D:\models e as pastas de apps de IA
// comuns (LM Studio, Ollama, etc.)
const SEARCH_ROOTS = [
    PROJECT_ROOT,                       // pasta do NEXA
    'D:\\models',                       // pasta principal de modelos do usuário
    'D:\\qwen-auto-learning',           // treinamentos customizados
    path.join(process.env.USERPROFILE, '.lmstudio', 'models'),
    path.join(process.env.USERPROFILE, '.ollama', 'models'),
    path.join(process.env.USERPROFILE, 'models')
].filter(r => r && r.length > 0);

// Pastas que sempre devem ser ignoradas (build, node_modules, etc.)
const IGNORE_DIRS = [
    'llama.cpp', 'llama.cpp-build', 'llama-src', 'llama-build', '.git',
    'node_modules', 'build', 'bin', 'dist'
];

// ============================================================================
// ESTADO
// ============================================================================

let llamaProcess = null;
let lastKnownGoodModel = '';

function stableGpuLayers(value = process.env.NEXA_GPU_LAYERS) {
    // Alguns drivers retornam tokens corrompidos com offload parcial. O modo
    // CPU é o padrão confiável; usuários que validarem sua GPU podem definir
    // NEXA_GPU_LAYERS com um inteiro não negativo.
    return /^\d+$/.test(String(value || '')) ? String(value) : '0';
}

// ============================================================================
// DESCOBERTA DE MODELOS
// ============================================================================

function isUsableModel(filePath) {
    const normalized = String(filePath || '').replace(/\\/g, '/').toLowerCase();
    const base = path.basename(normalized);
    const embeddingOnly = /(?:^|[-_.])(embed(?:ding)?|rerank(?:er|ing)?)(?:[-_.]|$)/i.test(base)
        || normalized.includes('/models/embeddings/')
        || /(?:^|[-_.])(bge|e5|gte)(?:[-_.]|$)/i.test(base);
    return base.endsWith('.gguf') && !base.startsWith('ggml-vocab') && !embeddingOnly;
}

// GGUF descreve o container, não garante que a arquitetura interna seja
// implementada pelo llama.cpp empacotado. Esta lista é conservadora: somente
// arquiteturas que já falharam de forma comprovada ficam bloqueadas. As demais
// continuam disponíveis e são protegidas pelo rollback atômico ao carregar.
function assessCompatibility(filePath) {
    const name = path.basename(String(filePath || '')).toLowerCase();
    if (/deepseek[-_.]?v4.*dspark|dspark.*deepseek[-_.]?v4/.test(name)) {
        return {
            status: 'unsupported',
            reason: 'A arquitetura deepseek4-dspark não é suportada pelo motor local incluído.'
        };
    }
    return { status: 'unknown', reason: 'Compatibilidade será confirmada ao carregar; o modelo atual é preservado se falhar.' };
}

// ============================================================================
// HARDWARE
// ============================================================================

let hardwareProfileCache = null;

function probeGpu() {
    try {
        const res = spawnSync('powershell',
            ['-NoProfile', '-Command', '(Get-CimInstance Win32_VideoController).Name'],
            { encoding: 'utf8', timeout: 4000, windowsHide: true });
        const names = String(res.stdout || '').split('\r\n')
            .map(s => s.trim()).filter(Boolean, /[a-z]/i);
        return names.length > 0 ? names.join(' + ') : null;
    } catch {
        return null;
    }
}

function getHardwareProfile() {
    if (hardwareProfileCache) return hardwareProfileCache;
    const cpus = os.cpus();
    const totalRamGb = os.totalmem() / (1024 ** 3);
    const freeRamGb = os.freemem() / (1024 ** 3);
    // Largura de banda efetiva de leitura da RAM em CPU: os modelos com MoE
    // (Q2 80GB) mediram ~0.45 tok/s, o que dá ~40 GB/s reais nesta máquina.
    // Modelos menores se beneficiam do cache; usa um teto mais otimista.
    const bandwidthGbPerSec = 40;
    hardwareProfileCache = {
        cpu: cpus.length ? cpus[0].model.trim() : 'desconhecida',
        cores: cpus.length,
        totalRamGb: Math.round(totalRamGb * 10) / 10,
        freeRamGb: Math.round(freeRamGb * 10) / 10,
        bandwidthGbPerSec,
        gpu: probeGpu() || 'nenhuma detectada (modo CPU)',
        listTokensPerSec: (sizeGb) => sizeGb > 0 ? bandwidthGbPerSec / sizeGb : null
    };
    return hardwareProfileCache;
}

// ============================================================================
// LEITURA DO HEADER GGUF (arquitetura, parâmetros, contexto)
// ============================================================================

const ggufMetaCache = new Map();

function readGgufMeta(filePath) {
    if (ggufMetaCache.has(filePath)) return ggufMetaCache.get(filePath);
    const meta = { arch: null, paramsB: null, sizeLabel: null, context: null };
    try {
        const fd = fs.openSync(filePath, 'r');
        try {
            const header = Buffer.alloc(24);
            if (fs.readSync(fd, header, 0, 24, 0) < 24) return meta;
            if (header.toString('ascii', 0, 4) !== 'GGUF') return meta;
            const kvCount = Number(header.readBigUInt64LE(16));
            if (kvCount > 100000) return meta;
            let offset = 24;
            const readAt = (size) => {
                const buf = Buffer.alloc(size);
                const n = fs.readSync(fd, buf, 0, size, offset);
                offset += size;
                return n === size ? buf : null;
            };
            const readU64 = () => {
                const buf = readAt(8);
                return buf ? Number(buf.readBigUInt64LE(0)) : null;
            };
            const readStr = () => {
                const len = readU64();
                if (len === null || len > 4096) return null;
                const buf = readAt(len);
                return buf ? buf.toString('utf8') : null;
            };
            for (let i = 0; i < kvCount; i++) {
                const key = readStr();
                if (key === null) break;
                const typeBuf = readAt(4);
                if (!typeBuf) break;
                const vtype = typeBuf.readUInt32LE(0);
                let value;
                switch (vtype) {
                    case 4: { const b = readAt(4); value = b ? b.readUInt32LE(0) : null; break; }
                    case 6: { const b = readAt(4); value = b ? b.readFloatLE(0) : null; break; }
                    case 7: { const b = readAt(1); value = b ? b[0] !== 0 : null; break; }
                    case 8: value = readStr(); break;
                    case 10: value = readU64(); break;
                    case 11: { const b = readAt(8); value = b ? Number(b.readBigInt64LE(0)) : null; break; }
                    case 2: { const b = readAt(2); value = b ? b.readUInt16LE(0) : null; break; }
                    case 3: { const b = readAt(2); value = b ? b.readInt16LE(0) : null; break; }
                    case 0: readAt(1); continue;
                    case 1: readAt(1); continue;
                    case 9: {
                        readAt(4); // tipo do array
                        const count = readU64() || 0;
                        for (let j = 0; j < Math.min(count, 4096); j++) readStr();
                        continue;
                    }
                    default:
                        readAt(1); continue;
                }
                if (value === null) continue;
                if (key === 'general.architecture') meta.arch = String(value);
                else if (key === 'general.size_label') meta.sizeLabel = String(value);
                else if (key === 'general.context_length' || key.endsWith('.context_length')) meta.context = value;
            }
            meta.paramsB = parseParamsB(meta.sizeLabel);
        } finally {
            fs.closeSync(fd);
        }
    } catch {
        // header ilegível: mantém meta vazia
    }
    ggufMetaCache.set(filePath, meta);
    return meta;
}

function parseParamsB(sizeLabel) {
    if (typeof sizeLabel !== 'string') return null;
    // "7.6B" ou "256x8.4B" (total x ativos em MoE)
    const moe = sizeLabel.match(/^(\d+(?:\.\d+)?)\s*[xX]\s*(\d+(?:\.\d+)?)\s*B/i);
    if (moe) return { totalB: parseFloat(moe[1]), activeB: parseFloat(moe[2]), moe: true };
    const plain = sizeLabel.match(/^(\d+(?:\.\d+)?)\s*B/i);
    if (plain) return { totalB: parseFloat(plain[1]), activeB: parseFloat(plain[1]), moe: false };
    return null;
}

// ============================================================================
// CLASSIFICAÇÃO DE DESEMPENHO (verde / laranja / vermelho)
// ============================================================================

const NON_LLM_ARCHS = new Set([
    'clip', 'flux', 'yolov3', 'audiocpp', 'parakeet', 'stable-diffusion',
    'sd', 'sdxl', 'whisper', 'wav2vec', 'canary', 'amuse', 'dit', 'vae'
]);

const NON_LLM_FILE_PATTERNS = [
    /(^|[-_.])lora([-_.]|$)/i,
    /(^|[-_.])(clip|flux|yolov\d*|audiocpp|parakeet|whisper|reranker?)[-_.]/i,
    /stable[-_.]?diffusion/i,
    /\.mmproj/i
];

function classifyModelKind(filePath, meta) {
    const base = path.basename(String(filePath || ''));
    if (meta.arch && NON_LLM_ARCHS.has(meta.arch)) return 'non-llm';
    if (NON_LLM_FILE_PATTERNS.some(r => r.test(base))) return 'non-llm';
    return 'llm';
}

function rateModelRun(filePath, sizeGB) {
    const meta = readGgufMeta(filePath);
    const hw = getHardwareProfile();
    const size = sizeGB || 0;
    const estTokPerSec = size > 0 ? hw.listTokensPerSec(size) : null;

    if (classifyModelKind(filePath, meta) === 'non-llm') {
        return {
            category: 'unsupported',
            reason: 'Este arquivo é um adaptador/ferramenta (ex.: LoRA, clip, flux, yolov, whisper) e não carrega como modelo de chat no llama-server.',
            color: 'gray',
            estTokensPerSec: null,
            fitsRam: null,
            meta
        };
    }

    if (size <= 0) {
        return {
            category: 'unknown',
            reason: 'Tamanho desconhecido.',
            color: 'gray',
            estTokensPerSec: null,
            meta
        };
    }

    // Folga de ~20% além do arquivo para KV cache + ativações em RAM.
    const workingSetGb = size * 1.25;
    const ramHeadroomGb = Math.max(0.5, hw.totalRamGb * 0.15);
    const fitsRam = workingSetGb <= (hw.totalRamGb - ramHeadroomGb);

    let category;
    let color;
    let reason;
    if (!fitsRam) {
        category = 'extreme';
        color = 'red';
        reason = `${size.toFixed(1)} GB de arquivo + folga (${workingSetGb.toFixed(1)} GB) excedem os ${hw.totalRamGb} GB de RAM desta máquina. Não vai rodar em CPU sem travar o sistema.`;
    } else if (estTokPerSec >= 12) {
        category = 'excellent';
        color = 'green';
        reason = `Fica na RAM (${workingSetGb.toFixed(1)} GB) e estima ~${estTokPerSec.toFixed(1)} tokens/s. Recomendado.`;
    } else if (estTokPerSec >= 5) {
        category = 'good';
        color = 'green';
        reason = `Fica na RAM (${workingSetGb.toFixed(1)} GB) e estima ~${estTokPerSec.toFixed(1)} tokens/s. Bom para o agente.`;
    } else if (estTokPerSec >= 2) {
        category = 'moderate';
        color = 'orange';
        reason = `Fica na RAM (${workingSetGb.toFixed(1)} GB), mas gera ~${estTokPerSec.toFixed(1)} tokens/s. Funciona, porém a criação de projetos ficará lenta.`;
    } else {
        category = 'extreme';
        color = 'red';
        reason = `Estima ~${estTokPerSec.toFixed(1)} tokens/s (${size.toFixed(1)} GB em CPU). Uso extremo: cada resposta leva minutos.`;
    }
    return { category, color, reason, estTokensPerSec: Math.round(estTokPerSec * 10) / 10, fitsRam, meta };
}

/**
 * Escaneia os diretórios raízes (recursivamente) por arquivos .gguf.
 * Ignora vocabs de teste, duplicatas e pastas de sistema/build.
 */
function listModels() {
    const seen = new Set();
    const models = [];
    const maxDepth = 8;

    function walk(dir, depth) {
        if (depth > maxDepth) return;
        let entries;
        try {
            entries = fs.readdirSync(dir, { withFileTypes: true });
        } catch {
            return;
        }
        for (const entry of entries) {
            const full = path.join(dir, entry.name);
            if (entry.isDirectory()) {
                if (IGNORE_DIRS.includes(entry.name)) continue;
                walk(full, depth + 1);
            } else if (entry.isFile() && isUsableModel(full)) {
                // Evita duplicatas (mesma IA em pastas diferentes)
                if (seen.has(entry.name)) continue;
                seen.add(entry.name);
                const compatibility = assessCompatibility(full);
                let fileSize = 0;
                try { fileSize = fs.statSync(full).size; } catch {}
                const sizeGB = fileSize > 0 ? Math.round((fileSize / (1024 ** 3)) * 100) / 100 : null;
                const performance = rateModelRun(full, sizeGB || 0);
                models.push({
                    id: full,
                    name: entry.name,
                    folder: path.basename(dir),
                    sizeGB,
                    compatibility,
                    performance: {
                        category: compatibility.status === 'unsupported' ? 'unsupported' : performance.category,
                        color: performance.color,
                        reason: compatibility.status === 'unsupported' ? compatibility.reason : performance.reason,
                        estTokensPerSec: performance.estTokensPerSec,
                        fitsRam: performance.fitsRam ?? null,
                        arch: performance.meta?.arch || null,
                        paramsB: performance.meta?.paramsB || null
                    }
                });
            }
        }
    }

    for (const root of SEARCH_ROOTS) {
        if (fs.existsSync(root)) {
            walk(root, 0);
        }
    }
    return models;
}

// ============================================================================
// SERVIDOR LLAMA
// ============================================================================

function findLlamaServer() {
    const candidates = [
        'C:\\Users\\Admin\\Downloads\\llama-b10453-bin-win-cpu-x64\\llama-server.exe',
        'C:\\llama-msvc-build\\_build\\bin\\Release\\llama-server.exe',
        path.join(PROJECT_ROOT, 'llama.cpp', 'bin', 'llama-server.exe'),
        path.join(PROJECT_ROOT, 'llama.cpp-build', 'bin', 'llama-server.exe'),
        path.join(PROJECT_ROOT, 'llama.cpp-build', 'source', 'bin', 'llama-server.exe'),
        path.join(PROJECT_ROOT, 'llama-src', 'llama-server.exe')
    ];
    for (const c of candidates) {
        if (fs.existsSync(c)) return c;
    }
    return null;
}

function stopLlamaServer() {
    return new Promise((resolve) => {
        // Encerra somente o processo criado por esta instância. Um servidor
        // externo na porta 8080 pertence ao usuário e nunca deve ser morto.
        const owned = llamaProcess;
        llamaProcess = null;
        if (!owned || owned.exitCode !== null) return resolve();
        const timeout = setTimeout(resolve, 2500);
        owned.once('exit', () => { clearTimeout(timeout); resolve(); });
        try { owned.kill(); } catch { clearTimeout(timeout); resolve(); }
    });
}

function startLlamaServer(modelPath) {
    return new Promise((resolve, reject) => {
        const exe = findLlamaServer();
        if (!exe) {
            reject(new Error('llama-server.exe não encontrado. Compile o llama.cpp.'));
            return;
        }
        if (!modelPath || !fs.existsSync(modelPath)) {
            reject(new Error('Caminho do modelo não encontrado: ' + modelPath));
            return;
        }

        const args = [
            '-m', modelPath,
            // Configuração estável para o hardware local e para o contexto
            // efetivamente usado pelo agente.
            '-c', '16384',
            '-np', '1',
            '--cache-ram', '0',
            '--no-cache-prompt',
            '--no-warmup',
            '--host', '127.0.0.1',
            '--port', '8080',
            '-ngl', stableGpuLayers(),
        ];
        // Alguns GGUFs carregam um template incompatível para /v1/chat/
        // completions. O DeepSeek Coder requer o template explícito.
        // Modelos Qwen precisam de --jinja para que o template processe
        // tool_calls corretamente via chat template.
        if (/deepseek-coder/i.test(path.basename(modelPath))) {
            args.push('--jinja', '--chat-template', 'deepseek');
        } else {
            args.push('--jinja');
        }

        const exeDir = require('path').dirname(exe);
        // Mantém o diagnóstico no backend.log do aplicativo. Antes, uma queda
        // do llama-server aparecia ao usuário apenas como "terminated", sem
        // a causa necessária para corrigi-la.
        llamaProcess = spawn(exe, args, { stdio: ['ignore', 'pipe', 'pipe'], cwd: exeDir });

        llamaProcess.stdout.on('data', chunk => console.log('[llama-server]', String(chunk).trim()));
        llamaProcess.stderr.on('data', chunk => console.warn('[llama-server]', String(chunk).trim()));
        llamaProcess.on('exit', (code, signal) => console.error(`[llama-server] encerrado: código=${code}, sinal=${signal || 'nenhum'}`));

        llamaProcess.on('error', (err) => {
            reject(new Error('Falha ao iniciar llama-server: ' + err.message));
        });

        const startedAt = Date.now();
        const interval = setInterval(async () => {
            try {
                const res = await fetch('http://127.0.0.1:8080/health', { timeout: 1000 });
                if (res.ok) {
                    clearInterval(interval);
                    lastKnownGoodModel = modelPath;
                    resolve(modelPath);
                }
            } catch {
                // ainda não subiu
            }
            if (Date.now() - startedAt > 180000) {
                clearInterval(interval);
                reject(new Error('Timeout: llama-server não respondeu em 3 minutos.'));
            }
        }, 1500);
    });
}

/**
 * Troca para outro modelo: para o llama-server atual e sobe com o novo.
 */
async function switchModel(modelPath, fallbackModel = '') {
    const restoreModel = fallbackModel || lastKnownGoodModel;
    await stopLlamaServer();
    healthCache = { ts: 0, ok: false };
    if (await isServerUp()) {
        throw new Error('Existe um servidor de IA externo ativo na porta 8080. Selecione-o como provedor ou encerre-o antes de trocar o modelo local.');
    }
    try {
        await startLlamaServer(modelPath);
        return modelPath;
    } catch (error) {
        // Um GGUF com arquitetura não suportada não pode deixar o chat sem
        // servidor. A seleção continua registrada na conversa, mas o último
        // modelo compatível volta a atender o projeto imediatamente.
        if (restoreModel && restoreModel !== modelPath && fs.existsSync(restoreModel)) {
            try { await startLlamaServer(restoreModel); } catch {}
        }
        throw error;
    }
}

// Cache da verificação de health para não disparar muitas chamadas
let healthCache = { ts: 0, ok: false };

/**
 * Verifica se o llama-server está servindo na porta 8080.
 */
async function isServerUp() {
    if (Date.now() - healthCache.ts < 2000) return healthCache.ok;
    try {
        const res = await fetch('http://127.0.0.1:8080/health', { signal: AbortSignal.timeout(2000) });
        healthCache = { ts: Date.now(), ok: res.ok };
        return res.ok;
    } catch {
        healthCache = { ts: Date.now(), ok: false };
        return false;
    }
}

async function getRunningModel(baseUrl = 'http://127.0.0.1:8080') {
    try {
        const response = await fetch(String(baseUrl).replace(/\/$/, '') + '/v1/models', { signal: AbortSignal.timeout(2500) });
        if (!response.ok) return '';
        const body = await response.json();
        const item = (Array.isArray(body.data) && body.data[0]) || (Array.isArray(body.models) && body.models[0]);
        return String(item?.id || item?.model || item?.name || '');
    } catch {
        return '';
    }
}

/**
 * Garante que o llama-server esteja rodando com o modelo indicado.
 * Se a porta 8080 já tiver um servidor OK, mantém; senão, inicia.
 * Retorna o caminho do modelo ativo.
 */
async function ensureRunning(modelPath) {
    const up = await isServerUp();
    if (up) {
        lastKnownGoodModel = modelPath || lastKnownGoodModel;
        return await getRunningModel() || modelPath;
    }
    // Nenhum servidor ativo: inicia com o modelo solicitado
    await startLlamaServer(modelPath);
    return modelPath;
}

module.exports = {
    listModels,
    stableGpuLayers,
    findLlamaServer,
    switchModel,
    stopLlamaServer,
    ensureRunning,
    isServerUp,
    getRunningModel,
    isUsableModel,
    assessCompatibility,
    readGgufMeta,
    parseParamsB,
    classifyModelKind,
    getHardwareProfile,
    rateModelRun,
    getProjectRoot: () => PROJECT_ROOT,
    getSearchRoots: () => SEARCH_ROOTS
};
