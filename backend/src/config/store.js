/**
 * Persistência de configuração do NEXA
 *
 * Salva/carrega as configurações de IA do backend em config.json,
 * permitindo que o painel de configurações do frontend persista
 * (provedor ativo, URL e modelo). Segredos nunca são gravados neste arquivo.
 */

const fs = require('fs');
const runtimePaths = require('./runtimePaths');

const CONFIG_FILE = runtimePaths.configFile();

// Configurações padrão
const DEFAULT_CONFIG = {
    provider: 'local',            // 'local' (llama.cpp) ou 'openai'
    local: {
        baseUrl: 'http://127.0.0.1:8080',
        model: 'qwen2.5-coder-1.5b-q8_0'
    },
    openai: {
        baseUrl: 'https://api.openai.com/v1',
        apiKey: '',
        model: 'gpt-3.5-turbo'
    },
    embeddings: {
        enabled: true,
        baseUrl: 'http://127.0.0.1:8081/v1',
        apiKey: '',
        model: 'default'
    }
};

function normalizeOpenAIBaseUrl(value) {
    const raw = String(value || '').trim();
    if (!raw) return raw;
    // O NEXA acrescenta /chat/completions ao fazer a chamada. Aceitamos o
    // endpoint completo por conveniência, mas persistimos sempre a URL-base.
    return raw
        .replace(/[\\/]+$/, '')
        .trim()
        .replace(/\/chat\/completions$/i, '');
}

function readConfig() {
    try {
        if (fs.existsSync(CONFIG_FILE)) {
            const raw = fs.readFileSync(CONFIG_FILE, 'utf-8');
            const parsed = JSON.parse(raw);
            // Faz merge com defaults para garantir campos faltantes
            return {
                ...DEFAULT_CONFIG,
                ...parsed,
                local: { ...DEFAULT_CONFIG.local, ...(parsed.local || {}) },
                openai: { ...DEFAULT_CONFIG.openai, ...(parsed.openai || {}), baseUrl: normalizeOpenAIBaseUrl((parsed.openai || {}).baseUrl || DEFAULT_CONFIG.openai.baseUrl) }
                ,embeddings: { ...DEFAULT_CONFIG.embeddings, ...(parsed.embeddings || {}) }
            };
        }
    } catch (error) {
        console.warn('Não foi possível ler config.json:', error.message);
    }
    return { ...DEFAULT_CONFIG };
}

function writeConfig(config) {
    try {
        runtimePaths.ensureUserDataRoot();
        const current = readConfig();
        const data = {
            ...current,
            ...config,
            local: { ...current.local, ...(config.local || {}) },
            openai: { ...current.openai, ...(config.openai || {}), baseUrl: normalizeOpenAIBaseUrl((config.openai || {}).baseUrl || current.openai.baseUrl), apiKey: '' },
            embeddings: { ...current.embeddings, ...(config.embeddings || {}), apiKey: '' }
        };
        fs.writeFileSync(CONFIG_FILE, JSON.stringify(data, null, 2), 'utf-8');
        return data;
    } catch (error) {
        console.error('Não foi possível salvar config.json:', error.message);
        throw error;
    }
}

module.exports = {
    DEFAULT_CONFIG,
    normalizeOpenAIBaseUrl,
    readConfig,
    writeConfig
};
