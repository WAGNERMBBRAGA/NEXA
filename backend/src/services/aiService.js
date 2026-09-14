const store = require('../config/store');
const runtimeSecrets = require('../config/runtimeSecrets');
const toolAdapter = require('./toolAdapter');
const ollamaService = require('./ollamaService');
const kimiK3Service = require('./kimiK3Service');
const savedConfig = store.readConfig();

const config = {
    local: {
        name: 'NEXA Local (llama.cpp)',
        type: 'local-llm',
        baseUrl: savedConfig.local.baseUrl || 'http://127.0.0.1:8080',
        model: savedConfig.local.model || '',
        status: 'inactive'
    },
    ollama: {
        name: 'Ollama',
        type: 'ollama',
        baseUrl: savedConfig.ollama?.baseUrl || ollamaService.DEFAULT_BASE_URL,
        model: savedConfig.ollama?.model || '',
        status: 'inactive'
    },
    kimiK3: {
        name: 'Kimi K3 (C99 Engine)',
        type: 'kimi-k3',
        modelDir: savedConfig.kimiK3?.modelDir || '',
        trunkDir: savedConfig.kimiK3?.trunkDir || '',
        preset: savedConfig.kimiK3?.preset || 'laptop',
        status: 'inactive'
    },
    openai: {
        name: 'OpenAI API',
        type: 'api-llm',
        baseUrl: savedConfig.openai.baseUrl || 'https://api.openai.com/v1',
        apiKey: runtimeSecrets.get().openaiApiKey || savedConfig.openai.apiKey || null,
        model: savedConfig.openai.model || 'gpt-3.5-turbo',
        status: 'inactive'
    }
};

let providers = [config.local, config.ollama, config.kimiK3, config.openai];

function orderedProviders() {
    const providerMap = {
        local: config.local,
        ollama: config.ollama,
        kimiK3: config.kimiK3,
        openai: config.openai
    };
    const active = providerMap[savedConfig.provider] || config.local;
    const others = Object.values(providerMap).filter(p => p !== active);
    return [active, ...others];
}

function applyConfig(newConfig) {
    if (newConfig.local) {
        config.local.baseUrl = newConfig.local.baseUrl || config.local.baseUrl;
        config.local.model = newConfig.local.model || config.local.model;
    }
    if (newConfig.ollama) {
        config.ollama.baseUrl = newConfig.ollama.baseUrl || config.ollama.baseUrl;
        config.ollama.model = newConfig.ollama.model || config.ollama.model;
    }
    if (newConfig.kimiK3) {
        config.kimiK3.modelDir = newConfig.kimiK3.modelDir || config.kimiK3.modelDir;
        config.kimiK3.trunkDir = newConfig.kimiK3.trunkDir || config.kimiK3.trunkDir;
        config.kimiK3.preset = newConfig.kimiK3.preset || config.kimiK3.preset;
    }
    if (newConfig.openai) {
        config.openai.baseUrl = newConfig.openai.baseUrl || config.openai.baseUrl;
        if (Object.prototype.hasOwnProperty.call(newConfig.openai, 'apiKey')) {
            config.openai.apiKey = String(newConfig.openai.apiKey || '') || null;
            runtimeSecrets.apply({ openaiApiKey: newConfig.openai.apiKey });
        }
        config.openai.model = newConfig.openai.model || config.openai.model;
    }
    if (newConfig.embeddings) {
        savedConfig.embeddings = { ...(savedConfig.embeddings || {}), ...newConfig.embeddings };
        if (Object.prototype.hasOwnProperty.call(newConfig.embeddings, 'apiKey')) {
            runtimeSecrets.apply({ embeddingsApiKey: newConfig.embeddings.apiKey });
        }
    }
    savedConfig.provider = newConfig.provider || savedConfig.provider;
    statusCache.data = null;
}

function getActiveConfig() {
    return {
        provider: savedConfig.provider || 'local',
        local: { baseUrl: config.local.baseUrl, model: config.local.model },
        ollama: { baseUrl: config.ollama.baseUrl, model: config.ollama.model },
        kimiK3: { modelDir: config.kimiK3.modelDir, trunkDir: config.kimiK3.trunkDir, preset: config.kimiK3.preset },
        openai: { baseUrl: config.openai.baseUrl, model: config.openai.model, apiKey: config.openai.apiKey ? '***' : '' },
        embeddings: {
            enabled: !!(savedConfig.embeddings && savedConfig.embeddings.enabled),
            baseUrl: (savedConfig.embeddings && savedConfig.embeddings.baseUrl) || 'http://127.0.0.1:8081/v1',
            model: (savedConfig.embeddings && savedConfig.embeddings.model) || '',
            apiKey: runtimeSecrets.get().embeddingsApiKey ? '***' : ''
        }
    };
}

const statusCache = { timestamp: 0, data: null };
const CACHE_TTL = 3000;
// Um chat que não retorna não pode bloquear a conversa indefinidamente. O
// limite também vale para o endpoint de compatibilidade /completion. O modelo
// local ativo gera ~11 tokens/s: mesmo um turno curto precisa de ~60-90s. Um
// timeout apertado cortava a geração no meio e o rollback desfazia o trabalho.
// O teto alto garante que uma resposta completa sempre caiba; a fluidez vem de
// poucos turnos (MAX_TURNS) e respostas enxutas, não de cortar geração no meio.
const CHAT_TIMEOUT_MS = 300000;
// O modelo local ativo tem contexto de 2048 tokens. Histórico grande fazia a
// pergunta atual ser truncada pelo servidor e gerava respostas sem relação.
const MAX_HISTORY_CHARS = 450;
const contextCache = { baseUrl: '', value: 2048, timestamp: 0 };
const AGENT_TOOLS = [
    { type: 'function', function: { name: 'audit_project', description: 'Analisa a estrutura e validações do projeto ativo.', parameters: { type: 'object', properties: {} } } },
    { type: 'function', function: { name: 'inspect_project', description: 'Cria um inventário estrutural e procura inconsistências comprováveis nos arquivos do projeto.', parameters: { type: 'object', properties: {} } } },
    { type: 'function', function: { name: 'index_project', description: 'Atualiza incrementalmente o índice contextual do projeto para pesquisas rápidas.', parameters: { type: 'object', properties: {} } } },
    { type: 'function', function: { name: 'build_semantic_index', description: 'Gera e persiste embeddings reais do projeto quando um provedor de embeddings está configurado.', parameters: { type: 'object', properties: {} } } },
    { type: 'function', function: { name: 'semantic_search', description: 'Localiza trechos semanticamente relacionados por similaridade vetorial.', parameters: { type: 'object', properties: { query: { type: 'string' }, limit: { type: 'number' } }, required: ['query'] } } },
    { type: 'function', function: { name: 'list_files', description: 'Lista a estrutura de arquivos do projeto ativo sem ler conteúdos.', parameters: { type: 'object', properties: { path: { type: 'string' } } } } },
    { type: 'function', function: { name: 'search_project', description: 'Pesquisa texto, símbolos ou mensagens de erro em todos os arquivos textuais do projeto.', parameters: { type: 'object', properties: { query: { type: 'string' }, path: { type: 'string' } }, required: ['query'] } } },
    { type: 'function', function: { name: 'read_file', description: 'Lê um arquivo relativo ao projeto ativo antes de analisá-lo ou editá-lo.', parameters: { type: 'object', properties: { path: { type: 'string' } }, required: ['path'] } } },
    { type: 'function', function: { name: 'write_file', description: 'Cria ou substitui um arquivo relativo ao projeto ativo.', parameters: { type: 'object', properties: { path: { type: 'string' }, content: { type: 'string' } }, required: ['path', 'content'] } } },
    { type: 'function', function: { name: 'create_file', description: 'Cria um arquivo novo relativo ao projeto ativo e falha se ele já existir.', parameters: { type: 'object', properties: { path: { type: 'string' }, content: { type: 'string' } }, required: ['path', 'content'] } } },
    { type: 'function', function: { name: 'replace_text', description: 'Substitui uma ocorrência exata em um arquivo já lido, sem reescrever o restante.', parameters: { type: 'object', properties: { path: { type: 'string' }, oldText: { type: 'string' }, newText: { type: 'string' } }, required: ['path', 'oldText', 'newText'] } } },
    { type: 'function', function: { name: 'run_command', description: 'Executa uma validação segura no projeto.', parameters: { type: 'object', properties: { command: { type: 'array', items: { type: 'string' } } }, required: ['command'] } } },
    { type: 'function', function: { name: 'create_project', description: 'Cria um novo projeto dentro da raiz autorizada.', parameters: { type: 'object', properties: { path: { type: 'string' }, files: { type: 'array' } }, required: ['path'] } } },
    { type: 'function', function: { name: 'document_development_modes', description: 'Documenta com segurança os modos local e Docker sem alterar banco ou serviços.', parameters: { type: 'object', properties: {} } } }
];

function agentToolsFor(names) {
    if (!Array.isArray(names) || names.length === 0) return AGENT_TOOLS;
    const allowed = new Set(names);
    const selected = AGENT_TOOLS.filter(tool => allowed.has(tool.function.name));
    // Alguns provedores recusam `tools: []`; nesse caso preservamos o
    // contrato completo em vez de enviar uma lista vazia.
    return selected.length ? selected : AGENT_TOOLS;
}

function structuredToolInstructions() {
    return `\n\nYou are a file-operations agent for NEXA. Your ONLY allowed output is a single :::NEXA_ACTIONS block. Never write explanations, code, markdown or conversation. Produce exactly:\n:::NEXA_ACTIONS\n{"actions":[{"kind":"create_file","path":"P","content":"C"}]}\n:::\nValid kinds: create_file, write_file, read_file, replace_text, list_files, run_command, inspect_project. Use relative paths.`;
}

// Uma conversa de agente precisa manter a pergunta, a resposta e o resultado
// verificado mais recentes. Antes, o cliente gravava o histórico corretamente,
// mas esta camada enviava apenas a última mensagem do usuário ao modelo. Isso
// fazia "prossiga" perder o que havia sido analisado ou alterado na etapa
// anterior. O limite continua por caracteres para modelos locais pequenos.
function buildConversationHistory(history, historyCharLimit) {
    const budget = Math.min(Math.max(Number(historyCharLimit) || MAX_HISTORY_CHARS, MAX_HISTORY_CHARS), 12000);
    const candidates = (Array.isArray(history) ? history : [])
        .filter(item => item && ['user', 'assistant'].includes(item.role) && item.content)
        .slice(-8);
    const selected = [];
    let remaining = budget;

    for (const item of [...candidates].reverse()) {
        if (remaining <= 0) break;
        const content = String(item.content);
        const maximum = Math.min(content.length, remaining, 2400);
        const truncated = content.length > maximum ? content.slice(0, maximum) + '\n[contexto anterior resumido]' : content;
        selected.unshift({ role: item.role, content: truncated });
        remaining -= maximum;
    }
    return selected;
}

function apiChatPayload(provider, messages, temperature, maxTokens, includeTools = true) {
    return {
        model: provider.model,
        messages,
        temperature: temperature || 0.7,
        max_tokens: Math.min(Math.max(Number(maxTokens) || 4096, 32), 16384),
        ...(includeTools ? { tools: AGENT_TOOLS, tool_choice: 'auto' } : {})
    };
}

function shouldUseNativeTools(capabilities, contextSize, requireAction) {
    return !!requireAction && !!(capabilities && capabilities.nativeTools)
        && process.env.NEXA_DISABLE_NATIVE_TOOLS !== '1';
}

// Alguns provedores OpenAI-compatíveis (ex.: roteia.ai) rejeitam o campo
// `tools` com HTTP 400, mesmo aceitando o chat normalmente. Uma vez que a
// API recusa tools, o NEXA lembra e segue usando o protocolo de texto
// (`:::NEXA_ACTIONS`) em todas as tentativas seguintes — inclusive nos
// retries do loop do agente, evitando rejeições repetidas.
const toolsRejectedHosts = new Set();

function markToolsRejected(host) {
    if (host) toolsRejectedHosts.add(host);
}

function hostOf(baseUrl) {
    try { return new URL(baseUrl).host; } catch { return ''; }
}

function shouldIncludeTools(provider, includeTools) {
    if (!includeTools || !provider || provider.type !== 'api-llm') return includeTools;
    // APIs externas (ex.: roteia.ai) já se provaram mais confiáveis com o
    // protocolo textual :::NEXA_ACTIONS: chamadas de tools nativas fazem o
    // modelo explorar (list_files/read_file/inspect_project) sem nunca chegar
    // à ação de escrita dentro do limite de turnos, e algumas rejeitam o
    // campo `tools` com 400. O protocolo de texto é determinístico e validado
    // de ponta a ponta. Ferramentas nativas ficam disponíveis apenas se o
    // operador pedir explicitamente via ambiente.
    if (process.env.NEXA_ENABLE_API_NATIVE_TOOLS === '1') {
        return !toolsRejectedHosts.has(hostOf(provider.baseUrl));
    }
    return false;
}

async function readStream(response, onToken) {
    const decoder = new TextDecoder();
    let buffer = ''; let content = '';
    for await (const chunk of response.body) {
        buffer += decoder.decode(chunk, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop();
        for (const line of lines) {
            if (!line.startsWith('data:')) continue;
            const payload = line.slice(5).trim();
            if (!payload || payload === '[DONE]') continue;
            try {
                const delta = JSON.parse(payload).choices?.[0]?.delta || {};
                const token = delta.content || '';
                if (token) { content += token; onToken(token); }
            } catch {}
        }
    }
    return content;
}

function isRunning(provider) {
    if (provider.type === 'ollama') {
        return ollamaService.isRunning(provider.baseUrl);
    }
    if (provider.type === 'kimi-k3') {
        return kimiK3Service.isAvailable().then(ok => ok);
    }
    const url = provider.type === 'local-llm' ? provider.baseUrl + '/health' : provider.baseUrl + '/models';
    const headers = provider.type === 'api-llm' && provider.apiKey
        ? { Authorization: 'Bearer ' + provider.apiKey }
        : {};
    const timeout = provider.type === 'api-llm' ? 8000 : 2000;
    return fetch(url, { method: 'GET', headers, signal: AbortSignal.timeout(timeout) })
        .then(r => r.ok).catch(() => false);
}

async function refreshStatus() {
    const now = Date.now();
    if (statusCache.data && (now - statusCache.timestamp) < CACHE_TTL) return statusCache.data;

    const [localOk, ollamaOk, kimiK3Available, openaiOk] = await Promise.all([
        isRunning(config.local),
        ollamaService.isRunning(config.ollama.baseUrl),
        kimiK3Service.isAvailable(),
        isRunning(config.openai)
    ]);

    config.local.status = localOk ? 'active' : 'inactive';
    config.ollama.status = ollamaOk ? 'active' : 'inactive';
    config.kimiK3.status = kimiK3Available ? 'available' : 'unavailable';
    config.openai.status = (openaiOk && config.openai.apiKey) ? 'active' : 'inactive';

    providers = orderedProviders();
    statusCache.data = providers;
    statusCache.timestamp = now;
    return providers;
}

async function initialize() {
    try { await refreshStatus(); } catch (e) { console.error('Init error:', e.message); }
    return providers;
}

function getProviders() { return providers; }
async function getProviderStatus() { return await refreshStatus(); }

async function getLocalModelProfile() {
    const capabilities = await toolAdapter.getCapabilities(config.local.baseUrl);
    return toolAdapter.modelProfile(config.local.model, capabilities);
}

async function getAvailableModels(providerPreference) {
    const provider = providerForPreference(providerPreference || savedConfig.provider);
    try {
        if (provider.type === 'ollama') {
            const models = await ollamaService.listModels(provider.baseUrl);
            if (models.length === 0) return [{ name: provider.model || 'unknown', parameters: 'ollama' }];
            return models.map(m => ({
                name: m.name,
                parameters: 'ollama',
                size_gb: m.size_gb,
                modified: m.modified
            }));
        }
        if (provider.type === 'local-llm') {
            const res = await fetch(provider.baseUrl + '/v1/models', { method: 'GET', signal: AbortSignal.timeout(3000) });
            if (res.ok) {
                const data = await res.json();
                const list = Array.isArray(data.data) && data.data.length ? data.data : [];
                if (list.length === 0) return [{ name: provider.model || 'unknown', parameters: 'local' }];
                return list.map(m => ({
                    name: (m.id || m.name || '').split(/[\\/]/).pop() || 'unknown',
                    parameters: 'local'
                }));
            }
        }
        if (provider.type === 'kimi-k3') {
            return [{ name: 'Kimi K3 (2.78T)', parameters: 'kimi-k3', preset: config.kimiK3.preset }];
        }
    } catch {}
    return [{ name: provider.model || 'unknown', parameters: provider.type }];
}

async function getActiveModelId(baseUrl) {
    try {
        const res = await fetch(baseUrl + '/v1/models', { signal: AbortSignal.timeout(2000) });
        if (res.ok) {
            const data = await res.json();
            if (data.data && data.data.length > 0) return data.data[0].id;
        }
    } catch {}
    return 'default';
}

function estimateTokens(text) {
    if (!text) return 0;
    return Math.ceil(text.length / 3);
}

function truncateToFit(text, maxTokens) {
    if (!text || estimateTokens(text) <= maxTokens) return text;
    const chars = maxTokens * 3;
    return text.slice(0, chars) + '\n...[truncated]';
}

function buildCompletionPrompt(messages) {
    // Alguns servidores llama.cpp expõem somente /completion.  Esse formato
    // também funciona com os GGUFs DeepSeek Coder que não possuem template de
    // chat utilizável pelo endpoint OpenAI-compatible.
    const parts = [];
    for (const message of messages) {
        if (!message || !message.content) continue;
        if (message.role === 'assistant') {
            parts.push(`### Response:\n${message.content}`);
        } else if (message.role === 'system') {
            parts.push(`### Instruction:\n${message.content}`);
        } else {
            parts.push(`### Instruction:\n${message.content}`);
        }
    }
    parts.push('### Response:');
    return parts.join('\n\n');
}

function messagesForLocalChat(messages, capabilities) {
    // O template DeepSeek carregado pelo llama.cpp declara que não aceita o
    // papel system. Consolidar esse contexto na primeira pergunta evita o
    // erro "expected peg-native format" e preserva as instruções do agente.
    // A decisão vem das capacidades declaradas pelo servidor, não do nome do
    // modelo. Isso mantém o NEXA compatível quando a IA é trocada.
    if (capabilities.supportsSystem) return messages;
    const system = messages.filter(message => message && message.role === 'system')
        .map(message => message.content).join('\n\n');
    const rest = messages.filter(message => message && message.role !== 'system')
        .map(message => ({ role: message.role, content: message.content }));
    const firstUser = rest.find(message => message.role === 'user');
    if (firstUser && system) firstUser.content = `${system}\n\n---\n\n${firstUser.content}`;
    return rest.length ? rest : [{ role: 'user', content: system }];
}

function requestSignal(abortSignal) {
    const timeout = AbortSignal.timeout(CHAT_TIMEOUT_MS);
    return abortSignal ? AbortSignal.any([timeout, abortSignal]) : timeout;
}

async function localCompletion(provider, messages, temperature, abortSignal) {
    const response = await fetch(provider.baseUrl + '/completion', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        signal: requestSignal(abortSignal),
        body: JSON.stringify({
            prompt: buildCompletionPrompt(messages),
            stream: false,
            temperature: temperature || 0.7,
            n_predict: 8192,
            repeat_penalty: 1.1,
            stop: ['### Instruction:', '### Response:']
        })
    });
    if (!response.ok) {
        const text = await response.text().catch(() => '');
        throw new Error('llama-server completion HTTP ' + response.status + (text ? ': ' + text.slice(0, 200) : ''));
    }
    const data = await response.json();
    const content = String(data.content || '').trim();
    return { success: true, data: { content: content || 'No response.' } };
}

async function getContextSize(baseUrl) {
    if (contextCache.baseUrl === baseUrl && Date.now() - contextCache.timestamp < 5000) return contextCache.value;
    try {
        const res = await fetch(baseUrl + '/v1/models', { signal: AbortSignal.timeout(2000) });
        if (res.ok) {
            const data = await res.json();
            if (data.data && data.data[0] && data.data[0].meta && data.data[0].meta.n_ctx) {
                const value = Number(data.data[0].meta.n_ctx);
                contextCache.baseUrl = baseUrl;
                contextCache.value = value;
                contextCache.timestamp = Date.now();
                return value;
            }
        }
    } catch {}
    try {
        const res = await fetch(baseUrl + '/props', { signal: AbortSignal.timeout(2000) });
        if (res.ok) {
            const data = await res.json();
            const value = Number(data.n_ctx || (data.default_generation_settings && data.default_generation_settings.n_ctx));
            if (value > 0) {
                contextCache.baseUrl = baseUrl;
                contextCache.value = value;
                contextCache.timestamp = Date.now();
                return value;
            }
        }
    } catch {}
    return 2048;
}

function contextBudget(contextSize, actionRequest = false) {
    const window = Math.min(Math.max(Number(contextSize) || 2048, 1024), 131072);
    const responseTokens = actionRequest
        ? Math.min(1024, Math.max(256, Math.floor(window * 0.18)))
        : Math.min(2048, Math.max(512, Math.floor(window * 0.25)));
    return { contextSize: window, responseTokens, promptTokens: Math.max(640, window - responseTokens - 64) };
}

function providerForPreference(preference) {
    const map = {
        local: config.local,
        ollama: config.ollama,
        kimiK3: config.kimiK3,
        openai: config.openai
    };
    return map[preference] || config.local;
}

async function getActiveContextBudget(actionRequest = false, providerPreference) {
    const provider = providerForPreference(providerPreference || savedConfig.provider);
    const size = provider.type === 'local-llm' ? await getContextSize(provider.baseUrl) : 16384;
    return contextBudget(size, actionRequest);
}

async function chat(requestBody) {
        const { prompt, systemPrompt, providerPreference, modelPreference, temperature, history, memory, maxTokens, historyCharLimit, onToken, requireAction, toolNames, abortSignal } = requestBody;
        require('fs').appendFileSync('C:/Users/PROGRAMAS LOJA/AppData/Local/Temp/opencode/request.log', 'CHAT-ENTRY: requireAction=' + requireAction + ' provider=' + (providerPreference || savedConfig.provider) + ' prompt=' + String(prompt || '').slice(0, 80) + '\n');
        await refreshStatus();
        const configuredProvider = providerForPreference(providerPreference || savedConfig.provider);
        require('fs').appendFileSync('C:/Users/PROGRAMAS LOJA/AppData/Local/Temp/opencode/request.log', 'CHAT-PROVIDER: type=' + configuredProvider.type + ' status=' + configuredProvider.status + ' url=' + (configuredProvider.baseUrl || '') + '\n');
    const provider = modelPreference ? { ...configuredProvider, model: modelPreference } : configuredProvider;

    let sys = systemPrompt || 'You are NEXA AI, a helpful programming assistant. Always reply in Portuguese (Brazil).';
    if (memory && memory.trim()) {
        sys = sys + '\n\n[Long-term memory]\n' + memory.trim();
    }

    const messages = [{ role: 'system', content: sys }];
    messages.push(...buildConversationHistory(history, historyCharLimit));
    messages.push({ role: 'user', content: prompt || '' });

    try {
        if (provider.type === 'local-llm' && provider.status === 'active') {
            const capabilities = await toolAdapter.getCapabilities(provider.baseUrl);
            // Usa chamadas nativas quando o servidor as anuncia; caso contrário
            // mantém o protocolo textual universal. A variável existe apenas
            // como escape para um servidor incompatível, não como requisito de
            // configuração para o usuário.
            const localContextSize = await getContextSize(provider.baseUrl);
            const useNativeTools = shouldUseNativeTools(capabilities, localContextSize, requireAction);
            // Conversa normal nunca recebe protocolo de ferramentas. Esse
            // texto fazia modelos locais responderem com NEXA_ACTIONS, XML e
            // instruções internas mesmo quando o usuário só fazia uma pergunta.
            // A execução de projeto é conduzida pelo executor do NEXA; quando
            // necessário, apenas o caminho de ação pode receber ferramentas.
            const compatibleMessages = requireAction && !sys.includes(':::NEXA_ACTIONS')
                ? [{ role: 'system', content: sys + structuredToolInstructions() }, ...messages.slice(1)]
                : messages;
            const localMessages = messagesForLocalChat(compatibleMessages, capabilities);
            const streamResponse = typeof onToken === 'function';
            const requestBody = {
                    model: 'default',
                    messages: localMessages,
                    // O chat comum usa SSE para a resposta aparecer na bolha
                    // gradualmente. A execução de ferramentas permanece não
                    // streaming, pois cada resultado precisa ser validado.
                    stream: streamResponse,
                    temperature: temperature || 0.7,
                    // Respostas cotidianas devem ser ágeis. Etapas longas
                    // podem chamar ferramentas novamente, sem prender o chat
                    // aguardando centenas de tokens desnecessários.
                    n_predict: Math.min(Math.max(Number(maxTokens) || 4096, 32), 16384),
                    repeat_penalty: requireAction ? 1.0 : 1.1,
                    ...(requireAction ? {} : { tools: agentToolsFor(toolNames), tool_choice: 'auto' }),
                    // Modelos Qwen3 ativam o modo "thinking" por padrão e
                    // gastam ~50s e centenas de tokens raciocinando antes de
                    // produzir o bloco de ações. Em uma execução de agente o
                    // prompt já carrega o contexto; o thinking vira narrativa
                    // que corrói o n_predict e estoura os turnos. Mantemos o
                    // thinking desligado também nas ações. Templates sem esse
                    // kwarg apenas o ignoram de forma segura.
                    chat_template_kwargs: { enable_thinking: false }
                };
            console.log('[CHAT] Messages:', messages.length, '/ System prompt:', sys.length, 'chars', '/ requireAction:', requireAction, '/ useNativeTools:', useNativeTools, '/ Tools:', requestBody.tools ? requestBody.tools.length : 'none');
            try {
                require('fs').appendFileSync('C:/Users/PROGRAMAS LOJA/AppData/Local/Temp/opencode/request.log', new Date().toISOString() + '\n' + JSON.stringify({ system: sys.slice(0, 2000), user: (localMessages[localMessages.length - 1] || {}).content || '', n_predict: requestBody.n_predict, temp: requestBody.temperature }) + '\n\n');
            } catch {}
            const response = await fetch(provider.baseUrl + '/v1/chat/completions', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                signal: requestSignal(abortSignal),
                body: JSON.stringify(requestBody)
            });

            if (response.ok) {
                if (streamResponse) {
                    const content = await readStream(response, onToken);
                    return { success: true, data: { content: content.trim(), actions: [] } };
                }
                const data = await response.json();
                const msg = data.choices && data.choices[0] && data.choices[0].message;
                let content = (msg && msg.content) || '';
                if (!content.trim() && msg && msg.reasoning_content) {
                    content = msg.reasoning_content;
                }
                content = content.trim();
                try {
                    require('fs').appendFileSync('C:/Users/PROGRAMAS LOJA/AppData/Local/Temp/opencode/request.log', 'RESPONSE: ' + JSON.stringify(content.slice(0, 6000)) + '\ntool_calls: ' + JSON.stringify((msg && msg.tool_calls) || []) + '\n\n');
                } catch {}
                const nativeActions = toolAdapter.nativeActions(msg);
                const templateActions = nativeActions.length ? [] : toolAdapter.textActions(content);
                // Alguns templates anunciam suporte nativo mas devolvem a
                // chamada serializada em `content`. O NEXA reconhece ambos e
                // remove a marcação técnica antes de ela chegar ao chat.
                const visibleContent = templateActions.length
                    ? content.replace(/<tool_call>[\s\S]*?<\/tool_call>/gi, '').trim()
                    : content;
                return { success: true, data: { content: visibleContent || '', actions: [...nativeActions, ...templateActions] } };
            }
            const errText = await response.text().catch(() => '');
            // Servidores com modelos completion-only devolvem 500 quando
            // recebem a estrutura de chat. Não repassamos esse defeito ao
            // usuário: reaproveitamos a mesma conversa no endpoint correto.
            if (/peg-native format|expected.*format/i.test(errText)) {
                return await localCompletion(provider, localMessages, temperature, abortSignal);
            }
            throw new Error('llama-server HTTP ' + response.status + (errText ? ': ' + errText.slice(0, 200) : ''));
        }

        if (provider.type === 'api-llm' && provider.status === 'active') {
            const requestApi = async (targetProvider, includeTools) => {
                includeToolsForLog = includeTools;
                return fetch(targetProvider.baseUrl + '/chat/completions', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    'Authorization': 'Bearer ' + targetProvider.apiKey
                },
                signal: requestSignal(abortSignal),
                body: JSON.stringify({ ...apiChatPayload(targetProvider, messages, temperature, maxTokens, shouldIncludeTools(targetProvider, includeTools)), stream: typeof onToken === 'function' })
            });
            };
            // Conversas comuns não precisam declarar ferramentas. Alguns
            // provedores compatíveis com OpenAI rejeitam esse campo ou o
            // interpretam de forma diferente, mesmo quando o chat simples
            // funcionaria normalmente.
            let response = await requestApi(provider, !!requireAction);

            // Uma conversa mantém sua escolha de modelo para não mudar de
            // comportamento sem aviso. Porém, se a API negar esse modelo por
            // plano/permissão, o chat não deve ficar inutilizado: tenta o
            // modelo padrão já configurado para a mesma API.
            if (!response.ok && response.status === 403 && modelPreference && modelPreference !== configuredProvider.model) {
                response = await requestApi(configuredProvider, !!requireAction);
            }

            // Nem todo endpoint compatível com OpenAI implementa tools. Ele
            // continua utilizável como chat normal em vez de falhar por isso.
            if (!response.ok && [400, 404, 422].includes(response.status)) {
                markToolsRejected(hostOf(provider.baseUrl));
                response = await requestApi(configuredProvider, false);
            }

            if (response.ok) {
                if (typeof onToken === 'function') {
                    const content = await readStream(response, onToken);
                    return { success: true, data: { content, actions: [] } };
                }
                const data = await response.json();
                const msg = data.choices && data.choices[0] && data.choices[0].message;
                const content = (msg && msg.content) || '';
                try {
                    require('fs').appendFileSync('C:/Users/PROGRAMAS LOJA/AppData/Local/Temp/opencode/request.log', 'API-RESPONSE: ' + JSON.stringify(content.slice(0, 1500)) + '\ntool_calls: ' + JSON.stringify((msg && msg.tool_calls) || []) + ' includeTools=' + String(includeToolsForLog) + '\n\n');
                } catch {}
                return { success: true, data: { content, actions: toolAdapter.nativeActions(msg) } };
            }
            const detail = await response.text().catch(() => '');
            throw new Error('OpenAI HTTP ' + response.status + (detail ? ': ' + detail.slice(0, 240) : ''));
        }

        // Ollama provider
        if (provider.type === 'ollama' && provider.status === 'active') {
            const ollamaService = require('./ollamaService');
            const ollamaMessages = messages.map(m => ({ role: m.role, content: m.content }));
            const streamResponse = typeof onToken === 'function';

            try {
                const result = await ollamaService.chatCompletion(
                    provider.model,
                    ollamaMessages,
                    {
                        baseUrl: provider.baseUrl,
                        temperature: temperature || 0.7,
                        maxTokens: maxTokens || 4096,
                        stream: streamResponse
                    }
                );

                if (streamResponse) {
                    const decoder = new TextDecoder();
                    let content = '';
                    for await (const chunk of result) {
                        const text = decoder.decode(chunk, { stream: true });
                        const lines = text.split('\n').filter(Boolean);
                        for (const line of lines) {
                            try {
                                const data = JSON.parse(line);
                                if (data.message?.content) {
                                    content += data.message.content;
                                    onToken(data.message.content);
                                }
                            } catch {}
                        }
                    }
                    return { success: true, data: { content: content.trim(), actions: [] } };
                }

                return { success: true, data: { content: result.content || '', actions: [] } };
            } catch (err) {
                throw new Error('Ollama error: ' + err.message);
            }
        }

        // Kimi K3 provider
        if (provider.type === 'kimi-k3') {
            const kimiK3Service = require('./kimiK3Service');
            const available = await kimiK3Service.isAvailable();
            if (!available) {
                return {
                    success: false,
                    error: 'Kimi K3 engine not found. Clone kimi-k3-in-c and build it first.'
                };
            }

            try {
                const result = await kimiK3Service.runInference({
                    modelDir: provider.modelDir,
                    trunkDir: provider.trunkDir,
                    prompt: prompt || '',
                    preset: provider.preset || 'laptop',
                    genTokens: maxTokens || 4096,
                    onToken: typeof onToken === 'function' ? onToken : undefined
                });

                return { success: true, data: { content: result.output || '', actions: [] } };
            } catch (err) {
                throw new Error('Kimi K3 error: ' + err.message);
            }
        }

        return {
            success: true,
            data: { content: 'IA not available. Start the NEXA llama-server (port 8080) or configure an external API.' }
        };
    } catch (error) {
        console.error('Chat error:', error.message);
        return {
            success: false,
            error: 'Failed to access provider ' + provider.name + ': ' + error.message
        };
    }
}

module.exports = {
    initialize,
    getProviders,
    getProviderStatus,
    getAvailableModels,
    chat,
    applyConfig,
    getActiveConfig,
    getLocalModelProfile,
    buildCompletionPrompt,
    messagesForLocalChat,
    structuredToolInstructions,
    AGENT_TOOLS,
    agentToolsFor,
    contextBudget,
    shouldUseNativeTools,
    getActiveContextBudget,
    buildConversationHistory,
    apiChatPayload,
    readStream
};
