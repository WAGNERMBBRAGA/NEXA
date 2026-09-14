const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const projectService = require('./projectService');
const store = require('../config/store');
const runtimePaths = require('../config/runtimePaths');
const runtimeSecrets = require('../config/runtimeSecrets');

const INDEX_DIR = path.join(runtimePaths.dataDir(), 'vector-index');
const MAX_CHUNKS = 400;
const CHUNK_CHARS = 600;
const INDEX_VERSION = 4;
const EMBEDDING_BATCH_SIZE = 8;
const TEXT_EXTENSIONS = new Set(['.js', '.jsx', '.ts', '.tsx', '.py', '.php', '.rs', '.go', '.java', '.cs', '.c', '.cpp', '.h', '.hpp', '.vue', '.svelte', '.json', '.yml', '.yaml', '.toml', '.md', '.txt', '.sql', '.html', '.css']);

function indexFile(root) {
    const id = crypto.createHash('sha256').update(fs.realpathSync(root).toLocaleLowerCase()).digest('hex').slice(0, 24);
    return path.join(INDEX_DIR, `${id}.json`);
}

function embeddingConfig() {
    const config = store.readConfig().embeddings || {};
    return {
        enabled: config.enabled === true,
        baseUrl: String(config.baseUrl || '').replace(/\/$/, ''),
        model: String(config.model || ''),
        apiKey: runtimeSecrets.get().embeddingsApiKey || String(config.apiKey || '')
    };
}

function collectChunks(root) {
    const chunks = [];
    const fingerprintParts = [];
    for (const file of projectService.listAllFiles(root)) {
        if (chunks.length >= MAX_CHUNKS) break;
        const extension = path.extname(file.path).toLowerCase();
        const base = path.basename(file.path).toLowerCase();
        if (!TEXT_EXTENSIONS.has(extension) && !['dockerfile', 'requirements.txt'].includes(base)) continue;
        let stat;
        try { stat = fs.statSync(file.fullPath); } catch { continue; }
        if (stat.size > 512 * 1024) continue;
        fingerprintParts.push(`${file.path}:${stat.size}:${stat.mtimeMs}`);
        const result = projectService.readFileContent(file.fullPath);
        if (!result.content || result.binary || result.error) continue;
        if (result.content.includes('\0')) continue;
        for (let offset = 0; offset < result.content.length && chunks.length < MAX_CHUNKS; offset += CHUNK_CHARS) {
            const text = result.content.slice(offset, offset + CHUNK_CHARS);
            if (text.trim()) chunks.push({ path: file.path.replace(/\\/g, '/'), offset, text });
        }
    }
    return { chunks, fingerprint: crypto.createHash('sha256').update(fingerprintParts.join('|')).digest('hex') };
}

async function requestEmbeddings(config, input) {
    const preparedInput = /nomic/i.test(config.model) || /8081/.test(config.baseUrl)
        ? input.map(text => /^(search_query|search_document):/.test(text) ? text : `search_document: ${text}`)
        : input;
    // Use the OpenAI-compatible endpoint. llama.cpp's legacy /embeddings
    // endpoint has a different nested response shape and can conceal invalid values.
    const endpoint = /\/v1$/i.test(config.baseUrl)
        ? config.baseUrl + '/embeddings'
        : config.baseUrl + '/v1/embeddings';
    const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            ...(config.apiKey ? { Authorization: `Bearer ${config.apiKey}` } : {})
        },
        signal: AbortSignal.timeout(120000),
        body: JSON.stringify({ model: config.model, input: preparedInput })
    });
    if (!response.ok) throw new Error(`Servidor de embeddings respondeu HTTP ${response.status}.`);
    const body = await response.json();
    const vectors = (body.data || []).sort((a, b) => a.index - b.index).map(item => item.embedding);
    if (vectors.length !== input.length || vectors.some(vector => !Array.isArray(vector) || !vector.length || vector.some(value => !Number.isFinite(value)) || vector.every(value => value === 0))) {
        throw new Error('O servidor não retornou vetores válidos para todos os trechos.');
    }
    return vectors;
}

async function buildSemanticIndex(root) {
    const config = embeddingConfig();
    if (!config.enabled || !config.baseUrl || !config.model) {
        return { available: false, reason: 'Configure e ative um provedor de embeddings nas configurações da IA.' };
    }
    const source = collectChunks(root);
    const target = indexFile(root);
    if (fs.existsSync(target)) {
        try {
            const cached = JSON.parse(fs.readFileSync(target, 'utf8'));
            const validVectors = Array.isArray(cached.items) && cached.items.every(item => Array.isArray(item.vector) && item.vector.length && item.vector.every(Number.isFinite) && item.vector.some(value => value !== 0));
            if (validVectors && cached.version === INDEX_VERSION && cached.fingerprint === source.fingerprint && cached.model === config.model && cached.baseUrl === config.baseUrl) {
                return { available: true, cached: true, chunks: cached.items.length, dimensions: cached.dimensions };
            }
        } catch {}
    }
    const items = [];
    let skipped = 0;
    for (let offset = 0; offset < source.chunks.length; offset += EMBEDDING_BATCH_SIZE) {
        const batch = source.chunks.slice(offset, offset + EMBEDDING_BATCH_SIZE);
        try {
            const vectors = await requestEmbeddings(config, batch.map(item => item.text));
            batch.forEach((item, index) => items.push({ path: item.path, offset: item.offset, text: item.text.slice(0, 500), vector: vectors[index] }));
        } catch {
            skipped += batch.length;
        }
    }
    if (!items.length && source.chunks.length) throw new Error('O provedor não produziu nenhum vetor utilizável para este projeto.');
    const payload = { version: INDEX_VERSION, fingerprint: source.fingerprint, model: config.model, baseUrl: config.baseUrl, dimensions: items[0] ? items[0].vector.length : 0, items };
    fs.mkdirSync(INDEX_DIR, { recursive: true });
    const temporary = `${target}.${process.pid}.tmp`;
    fs.writeFileSync(temporary, JSON.stringify(payload), 'utf8');
    fs.renameSync(temporary, target);
    return { available: true, cached: false, chunks: items.length, skipped, dimensions: payload.dimensions };
}

function cosine(a, b) {
    let dot = 0; let aa = 0; let bb = 0;
    const length = Math.min(a.length, b.length);
    for (let i = 0; i < length; i++) { dot += a[i] * b[i]; aa += a[i] * a[i]; bb += b[i] * b[i]; }
    return aa && bb ? dot / Math.sqrt(aa * bb) : 0;
}

async function semanticSearch(root, query, limit = 10) {
    const built = await buildSemanticIndex(root);
    if (!built.available) return { ...built, query, matches: [] };
    const config = embeddingConfig();
    const queryInput = /nomic/i.test(config.model) || /8081/.test(config.baseUrl) ? `search_query: ${query}` : query;
    const [queryVector] = await requestEmbeddings(config, [queryInput]);
    const payload = JSON.parse(fs.readFileSync(indexFile(root), 'utf8'));
    const matches = payload.items.map(item => ({ path: item.path, offset: item.offset, score: cosine(queryVector, item.vector), preview: item.text }))
        .sort((a, b) => b.score - a.score).slice(0, Math.min(Math.max(limit, 1), 20));
    return { available: true, query, cached: built.cached, matches };
}

module.exports = { buildSemanticIndex, semanticSearch, requestEmbeddings, collectChunks, cosine };
