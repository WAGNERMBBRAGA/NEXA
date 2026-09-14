/**
 * Persistência opcional de segredos do backend standalone.
 *
 * O NEXA desktop grava segredos encriptados (safeStorage) no `secrets.json`
 * do userData e os injeta no backend via variáveis de ambiente. Já o backend
 * standalone (node backend/src/server.js) não possui esse processo pai, então
 * os segredos viviam apenas em memória e se perdiam a cada reinício.
 *
 * Este módulo dá ao modo standalone a mesma garantia: quando a chave é
 * aplicada via POST /api/config, ela é persistida em <userData>/data/secrets.json.
 * No modo desktop (NEXA_DESKTOP=1) nada é escrito aqui — a persistência fica
 * com o safeStorage do processo Electron.
 *
 * Segredos nunca são gravados em config.json.
 */
const fs = require('fs');
const path = require('path');
const runtimePaths = require('./runtimePaths');

const SECRETS_FILE = path.join(runtimePaths.dataDir(), 'secrets.json');

function isDesktop() {
    return process.env.NEXA_DESKTOP === '1';
}

function load() {
    if (isDesktop()) return { openaiApiKey: '', embeddingsApiKey: '' };
    try {
        if (fs.existsSync(SECRETS_FILE)) {
            const parsed = JSON.parse(fs.readFileSync(SECRETS_FILE, 'utf-8'));
            return {
                openaiApiKey: String(parsed.openaiApiKey || ''),
                embeddingsApiKey: String(parsed.embeddingsApiKey || '')
            };
        }
    } catch (error) {
        console.warn('Não foi possível ler data/secrets.json:', error.message);
    }
    return { openaiApiKey: '', embeddingsApiKey: '' };
}

function save(values = {}) {
    if (isDesktop()) return;
    try {
        runtimePaths.ensureUserDataRoot();
        const previous = load();
        const next = {
            openaiApiKey: Object.prototype.hasOwnProperty.call(values, 'openaiApiKey')
                ? String(values.openaiApiKey || '')
                : String(previous.openaiApiKey || ''),
            embeddingsApiKey: Object.prototype.hasOwnProperty.call(values, 'embeddingsApiKey')
                ? String(values.embeddingsApiKey || '')
                : String(previous.embeddingsApiKey || '')
        };
        fs.mkdirSync(runtimePaths.dataDir(), { recursive: true });
        fs.writeFileSync(SECRETS_FILE, JSON.stringify(next, null, 2), { encoding: 'utf-8', mode: 0o600 });
    } catch (error) {
        console.error('Não foi possível salvar data/secrets.json:', error.message);
    }
}

module.exports = { load, save, isDesktop };