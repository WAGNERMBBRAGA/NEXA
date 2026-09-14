/**
 * Conversation Store - NEXA
 *
 * Gerencia conversas persistentes em disco (arquivos JSON), permitindo:
 *  - Criar/listar/renomear/excluir conversas
 *  - Salvar o histórico de mensagens de cada conversa
 *  - Armazenar um resumo/memória por conversa ("onde o trabalho parou")
 */

const fs = require('fs');
const path = require('path');
const runtimePaths = require('../config/runtimePaths');

const DATA_DIR = runtimePaths.dataDir();
const CONV_DIR = path.join(DATA_DIR, 'conversations');

function ensureDirs() {
    if (!fs.existsSync(DATA_DIR)) fs.mkdirSync(DATA_DIR, { recursive: true });
    if (!fs.existsSync(CONV_DIR)) fs.mkdirSync(CONV_DIR, { recursive: true });
}

function convFile(id) {
    // Sanitiza o id para evitar path traversal
    const safe = String(id).replace(/[^a-zA-Z0-9_-]/g, '');
    return path.join(CONV_DIR, `${safe}.json`);
}

function sanitizeId(id) {
    return String(id).replace(/[^a-zA-Z0-9_-]/g, '');
}

/**
 * Cria uma nova conversa. Retorna o objeto da conversa.
 */
function createConversation(title, projectPath, projectName) {
    ensureDirs();
    const id = Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
    const conv = {
        id,
        title: (title || 'Nova conversa').slice(0, 120),
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        messages: [],
        memory: '',
        summary: '',
        taskState: null,
        workPlan: null,
        providerPreference: null,
        modelPreference: null,
        skillIds: null,
        workspaceId: null,
        projectPath: projectPath || null,
        projectName: projectName || null,
        language: null
    };
    fs.writeFileSync(convFile(id), JSON.stringify(conv, null, 2), 'utf-8');
    return conv;
}

/**
 * Lista todas as conversas (sem as mensagens, apenas metadados).
 */
function listConversations() {
    ensureDirs();
    const files = fs.existsSync(CONV_DIR) ? fs.readdirSync(CONV_DIR).filter(f => f.endsWith('.json')) : [];
    const convs = [];
    for (const file of files) {
        try {
            const raw = JSON.parse(fs.readFileSync(path.join(CONV_DIR, file), 'utf-8'));
            convs.push({
                id: raw.id,
                title: raw.title,
                createdAt: raw.createdAt,
                updatedAt: raw.updatedAt,
                messageCount: Array.isArray(raw.messages) ? raw.messages.length : 0,
                summary: raw.summary || '',
                hasMemory: !!(raw.memory && raw.memory.trim()),
                projectPath: raw.projectPath || null,
                projectName: raw.projectName || null
                ,workspaceId: raw.workspaceId || null
            });
        } catch { /* ignora corrompidos */ }
    }
    // Mais recentes primeiro
    convs.sort((a, b) => new Date(b.updatedAt) - new Date(a.updatedAt));
    return convs;
}

/**
 * Obtém uma conversa completa (com mensagens e memória).
 */
function getConversation(id) {
    const file = convFile(id);
    if (!fs.existsSync(file)) return null;
    try {
        return JSON.parse(fs.readFileSync(file, 'utf-8'));
    } catch {
        return null;
    }
}

/**
 * Salva a conversa completa em disco.
 */
function saveConversation(conv) {
    conv.updatedAt = new Date().toISOString();
    fs.writeFileSync(convFile(conv.id), JSON.stringify(conv, null, 2), 'utf-8');
    return conv;
}

/**
 * Adiciona uma mensagem a uma conversa (cria se não existir).
 */
function addMessage(convId, role, content, opts = {}) {
    const conv = getConversation(convId);
    if (!conv) throw new Error('Conversa não encontrada; o projeto vinculado foi preservado e nenhuma conversa vazia será criada.');
    // Se foi criada com id diferente do solicitado, mantém a nova
    const id = conv.id;
    conv.messages.push({
        id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
        role,
        content,
        images: opts.images || [],
        model: opts.model || null,
        actionResults: opts.actionResults || [],
        createdAt: new Date().toISOString()
    });
    saveConversation(conv);
    return conv;
}

/**
 * Exclui uma conversa.
 */
function deleteConversation(id) {
    const file = convFile(id);
    if (fs.existsSync(file)) {
        fs.unlinkSync(file);
        return true;
    }
    return false;
}

/**
 * Renomeia uma conversa.
 */
function renameConversation(id, newTitle) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.title = newTitle.slice(0, 120);
    saveConversation(conv);
    return conv;
}

function setProject(id, projectPath, projectName) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.projectPath = projectPath || null;
    conv.projectName = projectName || null;
    return saveConversation(conv);
}

function setWorkspace(id, workspaceId) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.workspaceId = workspaceId || null;
    return saveConversation(conv);
}

function setConversationPreferences(id, preferences = {}) {
    const conv = getConversation(id);
    if (!conv) return null;
    if (Object.prototype.hasOwnProperty.call(preferences, 'providerPreference')) {
        conv.providerPreference = preferences.providerPreference === 'openai' ? 'openai' : (preferences.providerPreference === 'local' ? 'local' : null);
    }
    if (Object.prototype.hasOwnProperty.call(preferences, 'modelPreference')) conv.modelPreference = preferences.modelPreference || null;
    if (Object.prototype.hasOwnProperty.call(preferences, 'skillIds')) conv.skillIds = Array.isArray(preferences.skillIds) ? [...new Set(preferences.skillIds.map(String))].slice(0, 30) : null;
    return saveConversation(conv);
}

/**
 * Define a memória de longo prazo de uma conversa.
 */
function setMemory(id, memory) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.memory = memory || '';
    saveConversation(conv);
    return conv;
}

/**
 * Define o resumo automático do estado do trabalho.
 */
function setSummary(id, summary) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.summary = summary || '';
    saveConversation(conv);
    return conv;
}

function setTaskState(id, taskState) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.taskState = taskState ? { ...taskState, updatedAt: new Date().toISOString() } : null;
    return saveConversation(conv);
}

function setWorkPlan(id, workPlan) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.workPlan = workPlan || null;
    return saveConversation(conv);
}

function setLanguage(id, language) {
    const conv = getConversation(id);
    if (!conv) return null;
    conv.language = language ? String(language).trim().slice(0, 40) || null : null;
    return saveConversation(conv);
}

module.exports = {
    createConversation,
    listConversations,
    getConversation,
    saveConversation,
    addMessage,
    deleteConversation,
    renameConversation,
    setProject,
    setWorkspace,
    setConversationPreferences,
    setMemory,
    setSummary,
    setTaskState,
    setWorkPlan,
    setLanguage,
    sanitizeId
};
