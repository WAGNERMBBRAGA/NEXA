/**
 * Ollama Service - NEXA
 *
 * Provedor de IA via Ollama. Ollama gerencia modelos localmente com suporte
 * a MoE (Mixture-of-Experts), expert streaming, e quantização GGUF.
 *
 * Modelos recomendados para NEXA:
 *   - qwen3.5:35b-a3b  (35B total, 3B ativos — MoE leve)
 *   - qwen3.5:122b-a10b (122B total, 10B ativos — MoE potente)
 *   - deepseek-v4-flash (284B total, 13B ativos — frontier)
 *   - codellama:7b      (7B — código)
 *   - qwen2.5-coder:7b  (7B — código)
 */

const { spawn } = require('child_process');
const path = require('path');

// ============================================================================
// CONFIG
// ============================================================================

const DEFAULT_BASE_URL = 'http://127.0.0.1:11434';
const HEALTH_TIMEOUT_MS = 3000;
const LIST_TIMEOUT_MS = 5000;

let ollamaProcess = null;

// ============================================================================
// HEALTH CHECK
// ============================================================================

async function isRunning(baseUrl) {
    const url = (baseUrl || DEFAULT_BASE_URL) + '/api/tags';
    try {
        const res = await fetch(url, { method: 'GET', signal: AbortSignal.timeout(HEALTH_TIMEOUT_MS) });
        return res.ok;
    } catch {
        return false;
    }
}

// ============================================================================
// LIST MODELS
// ============================================================================

async function listModels(baseUrl) {
    const url = (baseUrl || DEFAULT_BASE_URL) + '/api/tags';
    try {
        const res = await fetch(url, { method: 'GET', signal: AbortSignal.timeout(LIST_TIMEOUT_MS) });
        if (!res.ok) return [];
        const data = await res.json();
        return (data.models || []).map(m => ({
            name: m.name,
            size: m.size,
            size_gb: (m.size / (1024 ** 3)).toFixed(1),
            modified: m.modified_at,
            details: m.details || {}
        }));
    } catch {
        return [];
    }
}

// ============================================================================
// PULL MODEL
// ============================================================================

async function pullModel(modelName, baseUrl, onProgress) {
    const url = (baseUrl || DEFAULT_BASE_URL) + '/api/pull';
    const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: modelName, stream: true })
    });

    if (!res.ok) {
        throw new Error(`Ollama pull failed: ${res.status}`);
    }

    const decoder = new TextDecoder();
    let buffer = '';

    for await (const chunk of res.body) {
        buffer += decoder.decode(chunk, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop();

        for (const line of lines) {
            if (!line.trim()) continue;
            try {
                const status = JSON.parse(line);
                if (onProgress) onProgress(status);
            } catch {}
        }
    }

    return { ok: true };
}

// ============================================================================
// DELETE MODEL
// ============================================================================

async function deleteModel(modelName, baseUrl) {
    const url = (baseUrl || DEFAULT_BASE_URL) + '/api/delete';
    const res = await fetch(url, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: modelName })
    });
    return { ok: res.ok };
}

// ============================================================================
// CHAT COMPLETION (OpenAI-compatible via /api/chat)
// ============================================================================

async function chatCompletion(model, messages, options = {}) {
    const baseUrl = options.baseUrl || DEFAULT_BASE_URL;
    const url = baseUrl + '/api/chat';

    const body = {
        model,
        messages: messages.map(m => ({
            role: m.role,
            content: m.content
        })),
        stream: !!options.stream,
        options: {
            temperature: options.temperature || 0.7,
            num_predict: options.maxTokens || 4096,
            num_ctx: options.contextSize || 4096
        }
    };

    const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        signal: options.timeout ? AbortSignal.timeout(options.timeout) : undefined
    });

    if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new Error(`Ollama chat failed (${res.status}): ${text}`);
    }

    if (options.stream) {
        return res.body;
    }

    const data = await res.json();
    return {
        content: data.message?.content || '',
        model: data.model,
        done: data.done,
        total_duration: data.total_duration,
        eval_count: data.eval_count,
        eval_duration: data.eval_duration
    };
}

// ============================================================================
// START OLLAMA SERVER (se não estiver rodando)
// ============================================================================

function startOllama(ollamaPath) {
    if (ollamaProcess) return { alreadyRunning: true };

    const cmd = ollamaPath || 'ollama';
    ollamaProcess = spawn(cmd, ['serve'], {
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: true
    });

    ollamaProcess.unref();
    ollamaProcess.on('error', (err) => {
        console.error('Ollama start error:', err.message);
        ollamaProcess = null;
    });
    ollamaProcess.on('exit', () => { ollamaProcess = null; });

    return { started: true, pid: ollamaProcess.pid };
}

// ============================================================================
// STOP OLLAMA
// ============================================================================

function stopOllama() {
    if (ollamaProcess) {
        ollamaProcess.kill();
        ollamaProcess = null;
        return { stopped: true };
    }
    return { alreadyStopped: true };
}

// ============================================================================
// MODEL INFO
// ============================================================================

async function modelInfo(modelName, baseUrl) {
    const url = (baseUrl || DEFAULT_BASE_URL) + '/api/show';
    const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: modelName })
    });
    if (!res.ok) return null;
    return await res.json();
}

// ============================================================================
// EXPORTS
// ============================================================================

module.exports = {
    DEFAULT_BASE_URL,
    isRunning,
    listModels,
    pullModel,
    deleteModel,
    chatCompletion,
    startOllama,
    stopOllama,
    modelInfo
};
