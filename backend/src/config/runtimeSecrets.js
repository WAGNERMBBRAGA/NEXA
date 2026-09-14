const secretStore = require('./secretStore');

// O backend standalone persiste segredos em data/secrets.json. O modo desktop
// continua preferindo as variáveis de ambiente injetadas pelo processo
// Electron (safeStorage); o arquivo é o fallback para garantir que uma chave
// salva pelo frontend sobreviva a reinícios mesmo sem o desktop.
const persisted = secretStore.load();

const secrets = {
    openaiApiKey: String(process.env.OPENAI_API_KEY || persisted.openaiApiKey || ''),
    embeddingsApiKey: String(process.env.NEXA_EMBEDDINGS_API_KEY || persisted.embeddingsApiKey || '')
};

function apply(values = {}) {
    if (Object.prototype.hasOwnProperty.call(values, 'openaiApiKey')) {
        secrets.openaiApiKey = String(values.openaiApiKey || '');
    }
    if (Object.prototype.hasOwnProperty.call(values, 'embeddingsApiKey')) {
        secrets.embeddingsApiKey = String(values.embeddingsApiKey || '');
    }
    secretStore.save(secrets);
    return get();
}

function get() {
    return { ...secrets };
}

module.exports = { apply, get };
