/**
 * NEXA AI Assistant Application Logic
 */

// A versão desktop entrega um segredo efêmero no fragmento da URL. Fragmentos
// não são enviados ao servidor nem gravados em logs HTTP. Todas as chamadas
// internas passam a provar que vieram da janela aberta por esta instância.
const nexaApiTokenFromUrl = new URLSearchParams(window.location.hash.slice(1)).get('token');
if (nexaApiTokenFromUrl) {
    sessionStorage.setItem('nexa_api_token', nexaApiTokenFromUrl);
    history.replaceState(null, '', window.location.pathname + window.location.search);
}
const nexaApiToken = nexaApiTokenFromUrl || sessionStorage.getItem('nexa_api_token') || '';
const browserFetch = window.fetch.bind(window);
window.fetch = (resource, options = {}) => {
    const target = typeof resource === 'string' ? resource : resource.url;
    const isNexaApi = target.startsWith('/api/') || target.startsWith(window.location.origin + '/api/');
    if (!nexaApiToken || !isNexaApi) return browserFetch(resource, options);
    const headers = new Headers(options.headers || (typeof resource !== 'string' ? resource.headers : undefined));
    headers.set('X-NEXA-Token', nexaApiToken);
    return browserFetch(resource, { ...options, headers });
};

// ============================================================================
// APP STATE
// ============================================================================

const state = {
    messages: [],
    isLoading: false,
    statusMessage: '',
    provider: 'local',
    config: null,
    currentConversationId: null,
    conversations: [],
    workspaces: [],
    activeConversation: null,
    attachments: [],
    project: null // { name, path, tree, info }
};
let activitySource = null;
let activityElement = null;
let activityText = '';
let streamingMessageElement = null;
let activeMessageController = null;
let pendingActivityCleanup = null;


// ============================================================================
// DOM ELEMENTS
// ============================================================================

const messagesContainer = document.getElementById('messages');
const userInput = document.getElementById('user-input');
const chatForm = document.getElementById('chat-form');
const modelSelect = document.getElementById('model-select');
const providerSelect = document.getElementById('provider-select');
const aiStatusIndicator = document.getElementById('ai-status');
const btnNewChat = document.getElementById('btn-new-chat');
const btnLinkProject = document.getElementById('btn-link-project');
const convList = document.getElementById('conv-list');
const workspaceList = document.getElementById('workspace-list');
const btnRefreshConvs = document.getElementById('btn-refresh-convs');
const languageInput = document.getElementById('language-input');
const btnSuggestLanguage = document.getElementById('btn-suggest-language');

// Anexos
const fileInput = document.getElementById('file-input');
const attachmentsEl = document.getElementById('attachments');

// Memória
const memoryPanel = document.getElementById('memory-panel');
const memoryText = document.getElementById('memory-text');
const btnOpenMemory = document.getElementById('btn-open-memory');
const btnSaveMemory = document.getElementById('btn-save-memory');
const btnCloseMemory = document.getElementById('btn-close-memory');

// Modal elements
const btnConfig = document.getElementById('btn-config');
const configModal = document.getElementById('config-modal');
const btnCloseConfig = document.getElementById('btn-close-config');
const btnSaveConfig = document.getElementById('btn-save-config');
const cfgProvider = document.getElementById('cfg-provider');
const cfgLocalSection = document.getElementById('cfg-local-section');
const cfgOpenaiSection = document.getElementById('cfg-openai-section');
const cfgLocalUrl = document.getElementById('cfg-local-url');
const cfgLocalModel = document.getElementById('cfg-local-model');
const cfgOpenaiUrl = document.getElementById('cfg-openai-url');
const cfgOpenaiKey = document.getElementById('cfg-openai-key');
const cfgOpenaiModel = document.getElementById('cfg-openai-model');
const cfgEmbeddingEnabled = document.getElementById('cfg-embedding-enabled');
const cfgEmbeddingUrl = document.getElementById('cfg-embedding-url');
const cfgEmbeddingModel = document.getElementById('cfg-embedding-model');
const cfgEmbeddingKey = document.getElementById('cfg-embedding-key');
const cfgStatus = document.getElementById('cfg-status');

// ============================================================================
// INITIALIZATION
// ============================================================================

async function init() {
    try {
        await loadConfig();
        await checkAIStatus();
        await fetchModels();
        await loadConversations();
        await loadWorkspaces();
        await loadSkills();
        const activeResponse = await fetch('/api/session/active');
        const activeData = await activeResponse.json();
        const activeId = activeData.success && activeData.data && activeData.data.conversationId;
        if (activeId && state.conversations.some(conversation => conversation.id === activeId)) {
            await openConversation(activeId);
        } else if (state.conversations.length > 0) {
            await openConversation(state.conversations[0].id);
        } else {
            await newChat();
        }
        // Um projeto só existe no contexto da conversa à qual foi vinculado.
        // Nunca reabrimos um caminho antigo fora dela.
    } catch (error) {
        console.error('Init error:', error);
    }
}

// ============================================================================
// CONFIG LOAD / SAVE
// ============================================================================

async function loadConfig() {
    try {
        const res = await fetch('/api/config');
        const data = await res.json();
        if (data.success) {
            state.config = data.data;
            state.provider = (state.activeConversation && state.activeConversation.providerPreference) || data.data.provider || 'local';
            providerSelect.value = state.provider;

            // Preenche o modal
            cfgProvider.value = state.provider;
            cfgLocalUrl.value = data.data.local.baseUrl || 'http://127.0.0.1:8080';
            cfgLocalModel.value = data.data.local.model || '';
            modelSelect.value = data.data.local.model || '';
            cfgOpenaiUrl.value = data.data.openai.baseUrl || 'https://api.openai.com/v1';
            cfgOpenaiKey.value = data.data.openai.apiKey || '';
            cfgOpenaiModel.value = data.data.openai.model || 'gpt-3.5-turbo';

            const savedModel = localStorage.getItem('nexa_model_preference');
            const savedProvider = localStorage.getItem('nexa_provider_preference');
            if (savedModel) modelSelect.value = savedModel;
            if (savedProvider) { state.provider = savedProvider; providerSelect.value = savedProvider; cfgProvider.value = savedProvider; }
            cfgEmbeddingEnabled.checked = !!(data.data.embeddings && data.data.embeddings.enabled);
            cfgEmbeddingUrl.value = (data.data.embeddings && data.data.embeddings.baseUrl) || 'http://127.0.0.1:8081/v1';
            cfgEmbeddingModel.value = (data.data.embeddings && data.data.embeddings.model) || '';
            cfgEmbeddingKey.value = (data.data.embeddings && data.data.embeddings.apiKey) || '';
            syncConfigSections();
        }
    } catch (error) {
        console.warn('Não foi possível carregar config:', error.message);
    }
}

function syncConfigSections() {
    const isOpenai = cfgProvider.value === 'openai';
    cfgLocalSection.classList.toggle('hidden', isOpenai);
    cfgOpenaiSection.classList.toggle('hidden', !isOpenai);
}

async function saveConfig() {
    cfgStatus.textContent = 'Salvando...';
    cfgStatus.classList.remove('ok', 'err');
    try {
        const body = {
            provider: cfgProvider.value,
            local: {
                baseUrl: cfgLocalUrl.value.trim(),
                model: cfgLocalModel.value.trim()
            },
            openai: {
                baseUrl: cfgOpenaiUrl.value.trim(),
                ...(cfgOpenaiKey.value.trim() !== '***' ? { apiKey: cfgOpenaiKey.value.trim() } : {}),
                model: cfgOpenaiModel.value.trim()
            },
            embeddings: {
                enabled: cfgEmbeddingEnabled.checked,
                baseUrl: cfgEmbeddingUrl.value.trim(),
                model: cfgEmbeddingModel.value.trim(),
                ...(cfgEmbeddingKey.value.trim() !== '***' ? { apiKey: cfgEmbeddingKey.value.trim() } : {})
            }
        };
        if (window.nexaSecrets) {
            const secrets = {};
            if (cfgOpenaiKey.value.trim() !== '***') secrets.openaiApiKey = cfgOpenaiKey.value.trim();
            if (cfgEmbeddingKey.value.trim() !== '***') secrets.embeddingsApiKey = cfgEmbeddingKey.value.trim();
            if (Object.keys(secrets).length) await window.nexaSecrets.save(secrets);
        }
        const res = await fetch('/api/config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body)
        });
        const data = await res.json();
        if (data.success) {
            state.provider = body.provider;
            providerSelect.value = body.provider;
            // Sincroniza sidebar com config salva
            modelSelect.value = body.local.model;
            providerSelect.value = body.provider;
            cfgStatus.textContent = '✓ Configuração salva!';
            cfgStatus.classList.add('ok');
            // Recarrega modelos para o provedor ativo
            await fetchModels();
            await checkAIStatus();
        } else {
            cfgStatus.textContent = '✗ Erro ao salvar';
            cfgStatus.classList.add('err');
        }
    } catch (error) {
        cfgStatus.textContent = '✗ ' + error.message;
        cfgStatus.classList.add('err');
    }
    setTimeout(() => cfgStatus.textContent = '', 3000);
}

// ============================================================================
// AI STATUS CHECKER
// ============================================================================

async function checkAIStatus() {
    try {
        const response = await fetch('/api/chat/status');
        const data = await response.json();

        if (data.success && data.data.providers.length > 0) {
            const activeProvider = data.data.providers.find(p => (state.provider === 'openai' ? p.type === 'api-llm' : p.type === 'local-llm'));
            if (activeProvider) {
                let capability = activeProvider.type === 'local-llm' ? 'Local' : 'Nuvem';
                if (activeProvider.type === 'local-llm') {
                    try {
                        const profileResponse = await fetch('/api/models/active-profile');
                        const profileData = await profileResponse.json();
                        if (profileData.success) capability = profileData.data.nativeTools ? 'Agente completo' : 'Agente compatível';
                    } catch { /* status principal continua disponível */ }
                }
                aiStatusIndicator.textContent = `✅ ${activeProvider.name} · ${capability}`;
                aiStatusIndicator.classList.add('active');
                aiStatusIndicator.classList.remove('inactive');
            } else {
                aiStatusIndicator.textContent = '⚠️ IA indisponível';
                aiStatusIndicator.classList.remove('active');
                aiStatusIndicator.classList.add('inactive');
            }
        } else {
            aiStatusIndicator.textContent = '❌ Indisponível';
            aiStatusIndicator.classList.remove('active');
            aiStatusIndicator.classList.add('inactive');
        }
    } catch (error) {
        console.error('AI status check error:', error);
        aiStatusIndicator.textContent = '❌ Falha de conexão';
        aiStatusIndicator.classList.remove('active');
        aiStatusIndicator.classList.add('inactive');
    }
}

// ============================================================================
// MODEL FETCHER
// ============================================================================

// Lista todas as IAs (modelos .gguf) instaladas no computador
async function fetchModels() {
    try {
        const apiProvider = state.provider === 'openai';
        const response = await fetch(apiProvider ? '/api/chat/models?provider=openai' : '/api/models');
        const data = await response.json();

        if (data.success && data.data.length > 0) {
            document.getElementById('model-onboarding')?.classList.add('hidden');
            // Modelos locais são ordenados pelo tamanho; APIs preservam a ordem
            // devolvida pelo provedor para não alterar sua lista de modelos.
            const sorted = apiProvider ? data.data.slice() : data.data.slice().sort((a, b) => (a.sizeGB || 999) - (b.sizeGB || 999));

            // Preenche o select da sidebar (model-select)
            modelSelect.innerHTML = '';
            // Preenche o select do modal de config (cfg-local-model)
            cfgLocalModel.innerHTML = '';

            const perfColors = { green: '#22c55e', orange: '#f59e0b', red: '#ef4444', gray: '#888' };

            for (const model of sorted) {
                const incompatible = !apiProvider && model.compatibility && model.compatibility.status === 'unsupported';
                const perf = model.performance || {};
                const perfLabel = perf.category === 'excellent' ? ' [EXCELENTE]'
                    : perf.category === 'good' ? ' [BOM]'
                    : perf.category === 'moderate' ? ' [MODERADO]'
                    : perf.category === 'extreme' ? ' [USO EXTREMO]'
                    : perf.category === 'unsupported' ? ' [INCOMPATÍVEL]' : '';
                const perfColor = perfColors[perf.color] || '';
                const perfTip = perf.reason || '';
                const compatibilityTag = incompatible || perf.category === 'unsupported' ? ' [INCOMPATÍVEL]' : '';
                // Option para sidebar
                const optSidebar = document.createElement('option');
                optSidebar.value = model.id;
                optSidebar.dataset.name = model.name;
                optSidebar.title = perfTip;
                optSidebar.disabled = incompatible || perf.category === 'unsupported';
                if (incompatible || perf.category === 'unsupported') optSidebar.style.color = '#ef4444';
                else if (perfColor) optSidebar.style.color = perfColor;
                const size = model.sizeGB != null ? ` (${model.sizeGB}GB)` : '';
                const tag = apiProvider ? ' [API]' : perfLabel;
                optSidebar.textContent = `${model.name}${size}${tag}${compatibilityTag}`;
                modelSelect.appendChild(optSidebar);

                // Option para modal config
                const optCfg = document.createElement('option');
                optCfg.value = model.id;
                optCfg.title = perfTip;
                optCfg.disabled = incompatible || perf.category === 'unsupported';
                if (incompatible || perf.category === 'unsupported') optCfg.style.color = '#ef4444';
                else if (perfColor) optCfg.style.color = perfColor;
                optCfg.textContent = `${model.name}${size}${perfLabel}${compatibilityTag}`;
                cfgLocalModel.appendChild(optCfg);
            }

            // Restaura valor selecionado em AMBOS os selects
            const savedModel = localStorage.getItem('nexa_model_preference');
            const preferredModel = (state.activeConversation && state.activeConversation.modelPreference) || savedModel || (apiProvider ? state.config?.openai?.model : state.config?.local?.model);
            if (preferredModel) {
                cfgLocalModel.value = preferredModel;
                modelSelect.value = preferredModel;
            }
        } else {
            document.getElementById('model-onboarding')?.classList.remove('hidden');
            modelSelect.innerHTML = '<option value="">Nenhum modelo encontrado</option>';
            cfgLocalModel.innerHTML = '<option value="">Nenhum modelo encontrado</option>';
            aiStatusIndicator.textContent = '⚠️ Nenhum modelo .gguf no computador';
            aiStatusIndicator.classList.add('inactive');
            aiStatusIndicator.classList.remove('active');
        }
    } catch (error) {
        console.error('Model fetch error:', error);
        modelSelect.innerHTML = '<option value="">Erro ao carregar modelos</option>';
        cfgLocalModel.innerHTML = '<option value="">Erro ao carregar modelos</option>';
    }
}

// Troca de IA: reinicia o llama-server com o modelo escolhido
async function switchModel(modelPath) {
    if (state.provider === 'openai') {
        if (!state.currentConversationId) return;
        const response = await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}`, {
            method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ modelPreference: modelPath })
        });
        const data = await response.json();
        if (!data.success) throw new Error(data.error || 'Não foi possível selecionar o modelo da API.');
        state.activeConversation = data.data;
        localStorage.setItem('nexa_model_preference', modelPath);
        localStorage.setItem('nexa_provider_preference', state.provider);
        aiStatusIndicator.textContent = '✅ Modelo da API selecionado';
        aiStatusIndicator.classList.add('active');
        return;
    }
    const btn = document.getElementById('send-btn');
    const prevLabel = btn.textContent;
    btn.textContent = '⏳';
    btn.disabled = true;
    aiStatusIndicator.textContent = '⏳ Carregando modelo...';
    aiStatusIndicator.classList.remove('active', 'inactive');
    try {
        const response = await fetch('/api/models/switch', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: modelPath })
        });
        const data = await response.json();
        if (data.success) {
            aiStatusIndicator.textContent = '✅ Modelo carregado';
            aiStatusIndicator.classList.add('active');
            localStorage.setItem('nexa_model_preference', modelPath);
            localStorage.setItem('nexa_provider_preference', state.provider);
            if (state.config) {
                state.config.local.model = modelPath;
            }
            cfgLocalModel.value = modelPath;
            if (state.currentConversationId) {
                await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ modelPreference: modelPath }) });
                if (state.activeConversation) state.activeConversation.modelPreference = modelPath;
            }
            syncConfigSections();
        } else {
            aiStatusIndicator.textContent = '⚠️ ' + (data.error || 'Falha ao trocar modelo');
            aiStatusIndicator.classList.add('inactive');
        }
        await checkAIStatus();
    } catch (error) {
        console.error('Switch model error:', error);
        aiStatusIndicator.textContent = '❌ Falha ao trocar modelo';
        aiStatusIndicator.classList.add('inactive');
    } finally {
        btn.textContent = prevLabel;
        btn.disabled = false;
    }
}

// ============================================================================
// MESSAGE HANDLER
// ============================================================================

function renderMarkdown(text) {
    let html = escapeHtml(text);
    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (m, lang, code) => {
        return `<pre style="background:#f1f5f9;color:#1e293b;padding:12px;border-radius:6px;overflow-x:auto;font-size:13px;margin:8px 0;border:1px solid #e2e8f0"><code>${code}</code></pre>`;
    });
    html = html.replace(/`([^`]+)`/g, '<code style="color:#1e293b;font-family:monospace">$1</code>');
    html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/\n/g, '<br>');
    return html;
}

function addMessage(role, content, images) {
    const messageDiv = document.createElement('div');
    messageDiv.className = `message ${role}`;
    let inner = '';
    if (Array.isArray(images) && images.length > 0) {
        for (const img of images) {
            if (typeof img === 'string' && (img.startsWith('data:image/') || img.startsWith('http'))) {
                inner += `<img class="msg-image" src="${escapeHtml(img)}" alt="anexo">`;
            }
        }
    }

    inner += `<div class="message-content">${renderMarkdown(content)}</div>`;

    if (role === 'ai') {
        inner += `<div class="message-actions">
            <button class="msg-action-btn" title="Copiar resposta" onclick="copyMessageContent(this)">📋</button>
            <button class="msg-action-btn" title="Reenviar mensagem" onclick="retryMessage(this)">🔄</button>
        </div>`;
    }

    if (role === 'user') {
        inner += `<div class="message-actions">
            <button class="msg-action-btn" title="Editar e reenviar" onclick="editMessage(this)">✏️</button>
        </div>`;
    }

    messageDiv.innerHTML = inner;
    messageDiv.dataset.content = content;
    messagesContainer.appendChild(messageDiv);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
    return messageDiv;
}

function copyMessageContent(btn) {
    const msg = btn.closest('.message');
    const content = msg?.dataset.content || '';
    navigator.clipboard.writeText(content).then(() => {
        btn.textContent = '✅';
        setTimeout(() => btn.textContent = '📋', 1500);
    }).catch(() => {
        const textarea = document.createElement('textarea');
        textarea.value = content;
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
        btn.textContent = '✅';
        setTimeout(() => btn.textContent = '📋', 1500);
    });
}

function retryMessage(btn) {
    const msg = btn.closest('.message');
    const content = msg?.dataset.content || '';
    if (state.isLoading) return;
    const userMsg = msg.previousElementSibling;
    if (userMsg && userMsg.dataset.content) {
        sendPrompt(userMsg.dataset.content, []);
    }
}

function editMessage(btn) {
    const msg = btn.closest('.message');
    const content = msg?.dataset.content || '';
    userInput.value = content;
    userInput.focus();
    userInput.setSelectionRange(content.length, content.length);
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function isUnsafeHistoricalAssistantOutput(content) {
    const text = String(content || '');
    return /(?:rss\s*feed|<jupyter\s+output|edgekey|langchain|backup_project|tool function|internal format|<result>)/i.test(text);
}

function buildTreeString(tree, prefix, depth) {
    if (!tree || depth > 2) return '';
    let result = '';
    let count = 0;
    const items = Array.isArray(tree) ? tree : (tree.children || []);
    for (const item of items) {
        if (count >= 30) { result += prefix + '... (' + (items.length - count) + ' more)\n'; break; }
        const name = item.name || item;
        if (typeof item === 'object' && item.children) {
            result += prefix + name + '/\n';
            result += buildTreeString(item.children, prefix + '  ', depth + 1);
        } else {
            result += prefix + name + '\n';
        }
        count++;
    }
    return result;
}

function showProjectInSidebar(projectPath, projectName) {
    const section = document.getElementById('project-section');
    const info = document.getElementById('project-info-sidebar');
    const tree = document.getElementById('project-tree-sidebar');
    const chip = document.getElementById('active-project-chip');
    if (!projectPath) {
        section.classList.add('hidden');
        if (chip) { chip.textContent = 'Sem projeto vinculado'; chip.title = 'Esta conversa ainda não possui uma pasta de projeto.'; chip.classList.remove('bound'); }
        return;
    }
    section.classList.remove('hidden');
    const name = projectName || projectPath.split(/[\\/]/).pop();
    if (chip) { chip.textContent = '📁 ' + name; chip.title = projectPath; chip.classList.add('bound'); }
    const workspace = state.activeConversation && state.activeConversation.workspaceId
        ? state.workspaces.find(item => item.id === state.activeConversation.workspaceId)
        : null;
    const instructions = workspace ? workspace.instructions || '' : '';
    info.innerHTML = '<div class="proj-name">' + escapeHtml(name) + '</div><div class="proj-path">' + escapeHtml(projectPath) + '</div>'
        + (workspace ? '<details class="project-instructions"><summary>Instruções compartilhadas do projeto</summary><textarea id="workspace-instructions" maxlength="4000" placeholder="Ex.: use TypeScript, mantenha os testes atualizados e responda em português.">' + escapeHtml(instructions) + '</textarea><button type="button" id="save-workspace-instructions" class="workspace-save">Salvar instruções</button></details>' : '');

    const saveInstructions = document.getElementById('save-workspace-instructions');
    if (saveInstructions && workspace) {
        saveInstructions.addEventListener('click', async () => {
            const textarea = document.getElementById('workspace-instructions');
            const value = textarea ? textarea.value : '';
            saveInstructions.disabled = true;
            saveInstructions.textContent = 'Salvando…';
            try {
                const response = await fetch('/api/workspaces/' + encodeURIComponent(workspace.id), {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ instructions: value })
                });
                const data = await response.json();
                if (!data.success) throw new Error(data.error || 'Não foi possível salvar.');
                state.workspaces = state.workspaces.map(item => item.id === workspace.id ? data.data : item);
                saveInstructions.textContent = 'Salvo ✓';
                renderWorkspaceList();
            } catch (error) {
                saveInstructions.textContent = 'Tentar novamente';
            } finally {
                saveInstructions.disabled = false;
            }
        });
    }

    tree.innerHTML = '<div style="padding:4px 8px;color:#7c3aed;font-size:0.75rem">Carregando...</div>';
    fetch('/api/projects/load', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: projectPath })
    }).then(r => r.json()).then(data => {
        if (data.success && data.data.tree) {
            tree.innerHTML = '';
            renderProjectTree(data.data.tree, tree, 0);
        }
    }).catch(() => {
        tree.innerHTML = '<div style="padding:4px 8px;color:#ef4444;font-size:0.75rem">Erro ao carregar</div>';
    });
}

function renderProjectTree(items, container, depth) {
    if (depth > 2) return;
    let count = 0;
    for (const item of items) {
        if (count >= 15) {
            const more = document.createElement('div');
            more.className = 'tree-item';
            more.innerHTML = '<span class="tree-icon">...</span><span class="tree-name">' + (items.length - count) + ' mais</span>';
            container.appendChild(more);
            break;
        }
        const el = document.createElement('div');
        el.className = 'tree-item';
        const icon = item.type === 'directory' ? '&#x1F4C1;' : (item.icon || '&#x1F4C4;');
        el.innerHTML = '<span class="tree-icon">' + icon + '</span><span class="tree-name">' + escapeHtml(item.name) + '</span>';
        container.appendChild(el);
        if (item.type === 'directory' && item.children) {
            renderProjectTree(item.children, container, depth + 1);
        }
        count++;
    }
}

// ============================================================================
// CHAT SUBMISSION HANDLER
// ============================================================================

async function handleSubmit(event) {
    event.preventDefault();
    if (state.isLoading) {
        cancelCurrentGeneration();
        return;
    }

    const prompt = userInput.value.trim();
    if (!prompt && state.attachments.length === 0) return;

    // Anexos: converte imagens em data URLs a serem enviadas ao modelo
    const imageAttachments = state.attachments
        .filter(a => a.isImage)
        .map(a => a.dataUrl);

    await sendPrompt(prompt, imageAttachments);
}

// Lê a linguagem escolhida no campo acima do composer. Vazia = não definida.
function getSelectedLanguage() {
    if (languageInput) return languageInput.value.trim();
    return '';
}

// Envia uma mensagem e renderiza a resposta. Também usado pelo botão
// "PRÓXIMA ETAPA", que dispensa o usuário de digitar "prossiga".
async function sendPrompt(prompt, imageAttachments) {
    // Garante que existe uma conversa ativa
    if (!state.currentConversationId) {
        await createNewConversationOnServer();
    }

    const welcomeEl = messagesContainer.querySelector('.welcome');
    if (welcomeEl) welcomeEl.remove();

    addMessage('user', prompt, imageAttachments);
    startConversationActivity(state.currentConversationId);
    userInput.value = '';
    userInput.disabled = true;
    state.isLoading = true;
    const sendButton = document.getElementById('send-btn');
    if (sendButton) { sendButton.textContent = '■'; sendButton.title = 'Interromper resposta'; }
    renderAttachmentBar();

    const loadingEl = document.createElement('div');
    loadingEl.className = 'message ai streaming';
    loadingEl.innerHTML = '<div class="message-content"><div class="thinking-indicator"><div class="thinking-dots"><span></span><span></span><span></span></div><span class="thinking-text">Pensando...</span></div></div>';
    messagesContainer.appendChild(loadingEl);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
    streamingMessageElement = loadingEl;

    try {
        const requestBody = {
            message: prompt,
            images: imageAttachments,
            language: getSelectedLanguage(),
            temperature: 0.5
        };
        const convId = state.currentConversationId;
        activeMessageController = new AbortController();
        const response = await fetch(`/api/conversations/${encodeURIComponent(convId)}/messages`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(requestBody),
            signal: activeMessageController.signal
        });
        const data = await response.json();

        if (data.success) {
            if (activityElement) { activityElement.remove(); activityElement = null; }
            const content = data.data.aiResponse || 'Resposta não disponível.';
            const streamed = activityText.trim().length > 0;
            if (streamed) {
                loadingEl.classList.remove('streaming');
                loadingEl.classList.add('completed');
                loadingEl.querySelector('.message-content').innerHTML = renderMarkdown(content);
                loadingEl.dataset.content = content;
                messagesContainer.scrollTop = messagesContainer.scrollHeight;
            } else {
                loadingEl.remove();
                addMessage('ai', content);
            }
            state.activeConversation = data.data.conversation || state.activeConversation;
            state.messages.push({ role: 'user', content: prompt, images: imageAttachments });
            state.messages.push({ role: 'ai', content });
            state.attachments = [];
            renderAttachmentBar();
            const actionResults = data.data.actionResults || [];
            setTimeout(() => renderInlineCodeDiff(actionResults), 2500);
            if (actionResults.some(result => result.ok && ['write_file', 'create_file', 'replace_text', 'create_project', 'document_development_modes'].includes(result.kind))) {
                const activeConversation = data.data.conversation;
                if (activeConversation && activeConversation.projectPath) {
                    showProjectInSidebar(activeConversation.projectPath, activeConversation.projectName);
                }
            }
            renderNextStepButton(data);
            await loadConversations();
        } else {
            loadingEl.remove();
            addMessage('ai', `⚠️ ${data.error || 'Erro ao processar sua mensagem.'}`);
        }
    } catch (error) {
        console.error('Chat error:', error);
        loadingEl.remove();
        if (error.name !== 'AbortError') addMessage('ai', `⚠️ Erro ao processar sua mensagem: ${error.message}`);
    } finally {
        if (pendingActivityCleanup) clearTimeout(pendingActivityCleanup);
        pendingActivityCleanup = setTimeout(() => {
            stopConversationActivity();
            pendingActivityCleanup = null;
        }, 4000);
        userInput.disabled = false;
        state.isLoading = false;
        activeMessageController = null;
        if (sendButton) { sendButton.textContent = '➤'; sendButton.title = 'Enviar'; }
        userInput.focus();
    }
}

function cancelCurrentGeneration() {
    if (!state.isLoading) return;
    const convId = state.currentConversationId;
    if (convId) {
        fetch(`/api/conversations/${encodeURIComponent(convId)}/cancel`, { method: 'POST' }).catch(() => {});
    }
    if (activeMessageController) activeMessageController.abort();
}

// Mostra um botão "PRÓXIMA ETAPA" abaixo da última resposta quando ela
// alterou o projeto. O clique envia "prossiga" (continuação verificável)
// sem o usuário digitar nada. Funciona para envio e para histórico.
function renderNextStepButton(data) {
    const conv = data && data.data && data.data.conversation;
    const actionResults = data && data.data && data.data.actionResults;
    if (!conv || !conv.projectPath) return;
    if (!Array.isArray(actionResults)) return;
    const mutated = actionResults.some(result => result.ok
        && ['write_file', 'create_file', 'replace_text', 'create_project'].includes(result.kind));
    if (!mutated) return;

    document.querySelectorAll('.next-step-button').forEach(el => el.remove());
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'next-step-button';
    btn.textContent = '➤ PRÓXIMA ETAPA';
    btn.title = 'Continuar construindo este projeto (envia "prossiga")';
    btn.addEventListener('click', () => {
        btn.disabled = true;
        btn.textContent = '▶ Construindo próximo passo…';
        sendPrompt('prossiga', []).catch(() => {
            btn.disabled = false;
            btn.textContent = '➤ PRÓXIMA ETAPA';
        });
    });
    messagesContainer.appendChild(btn);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
}

function startConversationActivity(conversationId) {
    stopConversationActivity();
    if (!conversationId || typeof EventSource === 'undefined') return;
    activityElement = document.createElement('div');
    activityElement.className = 'agent-activity';
    activityElement.innerHTML = '<span>NEXA está preparando a resposta...</span>';
    activityText = '';
    messagesContainer.appendChild(activityElement);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
    let retryCount = 0;
    const maxRetries = 3;
    function connect() {
        activitySource = new EventSource(`/api/conversations/${encodeURIComponent(conversationId)}/activity`);
        activitySource.onmessage = event => {
            retryCount = 0;
            try {
                const data = JSON.parse(event.data);
                console.log('[SSE RECEIVED]', data.type, data.path || data.label || '');
                if (data.type === 'token') {
                    activityText += data.text || '';
                    if (streamingMessageElement) {
                        const content = streamingMessageElement.querySelector('.message-content');
                        if (content) {
                            const thinkingIndicator = content.querySelector('.thinking-indicator');
                            if (thinkingIndicator) {
                                thinkingIndicator.remove();
                            }
                            content.innerHTML = renderMarkdown(activityText);
                        }
                        streamingMessageElement.dataset.content = activityText;
                        messagesContainer.scrollTop = messagesContainer.scrollHeight;
                    }
                    if (activityElement) activityElement.remove();
                    activityElement = null;
                } else if (data.type === 'file_write') {
                    console.log('[SSE FILE_WRITE]', data.path, 'content:', (data.content || '').length, 'chars');
                    renderFileWriteInProgress(data.kind, data.path, data.content);
                } else if (data.label) {
                    console.log('[NEXA SSE] label:', data.label);
                    if (activityElement) activityElement.textContent = data.label;
                }
            } catch (e) { console.error('[NEXA SSE] parse error:', e); }
        };
        activitySource.onerror = () => {
            activitySource.close();
            if (retryCount < maxRetries && state.isLoading) {
                retryCount++;
                setTimeout(connect, 2000 * retryCount);
            }
        };
    }
    connect();
}

function renderFileWriteInProgress(kind, path, content) {
    if (!content || !content.trim()) return;
    const existingEl = document.querySelector(`.file-write-in-progress[data-path="${CSS.escape(path)}"]`);
    if (existingEl) existingEl.remove();

    const fileName = path.split(/[\\/]/).pop();
    const lines = content.split('\n');
    const kindLabels = { create_file: '✨ Criando', write_file: '✏️ Editando', replace_text: '🔄 Corrigindo' };
    const kindLabel = kindLabels[kind] || '📝 Escrevendo';

    const diffDiv = document.createElement('div');
    diffDiv.className = 'inline-code-diff file-write-in-progress';
    diffDiv.dataset.path = path;

    const codeLines = lines.map((line, i) => {
        return `<div class="diff-line diff-add"><span class="diff-line-num">${i + 1}</span><span class="diff-line-text">${escapeHtml(line)}</span></div>`;
    }).join('');

    diffDiv.innerHTML = `
        <div class="diff-header">
            <span class="diff-icon">${kind === 'create_file' ? '✨' : '✏️'}</span>
            <span class="diff-filename">${escapeHtml(fileName)}</span>
            <span class="diff-path">${escapeHtml(path)}</span>
            <span class="diff-badge ${kind === 'create_file' ? 'badge-create' : 'badge-edit'}">${kindLabel}</span>
            <span class="diff-streaming-dot"></span>
        </div>
        <div class="diff-body">${codeLines}</div>
    `;

    messagesContainer.appendChild(diffDiv);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
}

function stopConversationActivity() {
    if (activitySource) activitySource.close();
    activitySource = null;
    if (activityElement) activityElement.remove();
    activityElement = null;
    activityText = '';
    streamingMessageElement = null;
}

function renderAgentActionResults(results) {
    if (!Array.isArray(results) || results.length === 0) return;
    const lines = results.map(result => {
        const status = result.ok ? '✓' : '✗';
        const details = result.details || result;
        const target = details.path || (details.command && details.command.join(' '));
        const detail = result.ok ? (target || 'concluída') : result.error;
        let line = `${status} ${result.kind}: ${detail}`;
        if (result.kind === 'run_command') {
            line += ` (saída: ${details.exitCode === null || details.exitCode === undefined ? 'interrompido' : details.exitCode})`;
            const diagnostic = String(details.stderr || details.stdout || '').trim();
            if (diagnostic) line += `\n\`\`\`text\n${diagnostic.slice(0, 1200)}\n\`\`\``;
        }
        return line;
    });
    addMessage('ai', `Resultado real das ações:\n${lines.join('\n')}`);
}

function renderInlineCodeDiff(results) {
    console.log('[DIFF] called with', results.length, 'results');
    if (!Array.isArray(results)) return;
    const fileActions = results.filter(r => r.ok && ['write_file', 'create_file', 'replace_text'].includes(r.kind));
    if (fileActions.length === 0) return;

    for (const action of fileActions) {
        const details = action.details || action;
        const filePath = details.path || 'arquivo';
        const existingEl = document.querySelector(`.file-write-in-progress[data-path="${CSS.escape(filePath)}"]`);
        if (existingEl) {
            existingEl.classList.remove('file-write-in-progress');
            existingEl.classList.add('inline-code-diff');
            continue;
        }
        const fileName = filePath.split(/[\\/]/).pop();
        const preview = details.preview || '';
        const beforePreview = details.beforePreview || '';

        if (!preview && !beforePreview) continue;

        const diffDiv = document.createElement('div');
        diffDiv.className = 'inline-code-diff';

        let headerIcon = '📄';
        if (action.kind === 'create_file') headerIcon = '✨';
        else if (action.kind === 'replace_text') headerIcon = '✏️';

        let diffContent = '';
        if (beforePreview && preview) {
            const beforeLines = beforePreview.split('\n').filter(l => l.trim());
            const afterLines = preview.split('\n').filter(l => l.trim());
            diffContent = beforeLines.map(l => `<div class="diff-line diff-remove"><span class="diff-line-num">-</span><span class="diff-line-text">${escapeHtml(l)}</span></div>`).join('');
            diffContent += afterLines.map(l => `<div class="diff-line diff-add"><span class="diff-line-num">+</span><span class="diff-line-text">${escapeHtml(l)}</span></div>`).join('');
        } else if (preview) {
            const lines = preview.split('\n').filter(l => l.trim());
            diffContent = lines.map((l, i) => `<div class="diff-line diff-add"><span class="diff-line-num">${i + 1}</span><span class="diff-line-text">${escapeHtml(l)}</span></div>`).join('');
        }

        if (!diffContent) continue;

        diffDiv.innerHTML = `
            <div class="diff-header">
                <span class="diff-icon">${headerIcon}</span>
                <span class="diff-filename">${escapeHtml(fileName)}</span>
                <span class="diff-path">${escapeHtml(filePath)}</span>
                <span class="diff-badge ${action.kind === 'create_file' ? 'badge-create' : 'badge-edit'}">${action.kind === 'create_file' ? 'CRIADO' : 'EDITADO'}</span>
            </div>
            <div class="diff-body">${diffContent}</div>
        `;
        messagesContainer.appendChild(diffDiv);
    }
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
}

function renderActionCanvas() {
}

// ============================================================================
// EVENT LISTENERS
// ============================================================================

chatForm.addEventListener('submit', handleSubmit);

if (btnSuggestLanguage) {
    btnSuggestLanguage.addEventListener('click', async () => {
        const btn = btnSuggestLanguage;
        const previous = btn.textContent;
        btn.disabled = true;
        btn.textContent = 'Sugerindo...';
        try {
            const conversationId = state.currentConversationId;
            const objective = userInput.value.trim()
                || (state.activeConversation && state.activeConversation.workPlan && state.activeConversation.workPlan.objective)
                || (state.messages.length ? state.messages[state.messages.length - 1].content : '');
            if (!conversationId) {
                await createNewConversationOnServer();
            }
            if (!objective) {
                btn.textContent = 'Descreva primeiro o que criar';
            } else {
                const response = await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}/language-suggestion`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ objective })
                });
                const data = await response.json();
                if (data.success && data.data && data.data.language) {
                    if (languageInput) languageInput.value = data.data.language;
                    addMessage('ai', `💡 Sugestão de linguagem: **${data.data.language}** — ${data.data.reason}\n\nVocê pode ajustar o campo "Linguagem do projeto" antes de enviar.`);
                    messagesContainer.scrollTop = messagesContainer.scrollHeight;
                } else {
                    btn.textContent = 'Não consegui sugerir; use o campo';
                }
            }
        } catch (error) {
            console.error('Erro ao sugerir linguagem:', error);
            btn.textContent = 'Erro ao sugerir';
        } finally {
            setTimeout(() => {
                btn.disabled = false;
                btn.textContent = previous;
            }, 2500);
        }
    });
}

userInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        chatForm.dispatchEvent(new Event('submit'));
    }
});

userInput.addEventListener('input', () => {
    userInput.style.height = 'auto';
    userInput.style.height = Math.min(userInput.scrollHeight, 160) + 'px';
});

providerSelect.addEventListener('change', async (e) => {
    state.provider = e.target.value;
    cfgProvider.value = state.provider;
    localStorage.setItem('nexa_provider_preference', state.provider);
    try {
        if (!state.currentConversationId) throw new Error('Abra uma conversa antes de escolher a IA.');
        const response = await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ providerPreference: state.provider })
        });
        const data = await response.json();
        if (!data.success) throw new Error(data.error || 'Não foi possível salvar o provedor.');
        state.activeConversation = data.data;
        await checkAIStatus();
    } catch (error) {
        console.error('Provider change error:', error);
        aiStatusIndicator.textContent = '⚠️ Falha ao selecionar provedor';
        aiStatusIndicator.classList.add('inactive');
    }
    fetchModels();
});

modelSelect.addEventListener('change', (e) => {
    const selectedId = e.target.value;
    if (!selectedId) return;
    switchModel(selectedId);
});

btnNewChat.addEventListener('click', () => {
    newChat();
});

btnLinkProject.addEventListener('click', () => {
    attachProjectToCurrentConversation();
});

btnRefreshConvs.addEventListener('click', () => {
    loadConversations();
});

cfgProvider.addEventListener('change', syncConfigSections);

btnConfig.addEventListener('click', () => {
    configModal.classList.remove('hidden');
    loadConfigIntoModal();
});

function loadConfigIntoModal() {
    if (!state.config) return;
    cfgProvider.value = state.config.provider || 'local';
    cfgLocalUrl.value = state.config.local.baseUrl || 'http://127.0.0.1:8080';
    cfgLocalModel.value = state.config.local.model || 'qwen2.5-coder-1.5b-q8_0';
    cfgOpenaiUrl.value = state.config.openai.baseUrl || 'https://api.openai.com/v1';
    cfgOpenaiKey.value = state.config.openai.apiKey || '';
    cfgOpenaiModel.value = state.config.openai.model || 'gpt-3.5-turbo';
    cfgEmbeddingEnabled.checked = !!(state.config.embeddings && state.config.embeddings.enabled);
    cfgEmbeddingUrl.value = (state.config.embeddings && state.config.embeddings.baseUrl) || 'http://127.0.0.1:8081/v1';
    cfgEmbeddingModel.value = (state.config.embeddings && state.config.embeddings.model) || '';
    cfgEmbeddingKey.value = (state.config.embeddings && state.config.embeddings.apiKey) || '';
    syncConfigSections();
}

btnCloseConfig.addEventListener('click', () => {
    configModal.classList.add('hidden');
    cfgStatus.textContent = '';
});

configModal.addEventListener('click', (e) => {
    if (e.target === configModal) {
        configModal.classList.add('hidden');
        cfgStatus.textContent = '';
    }
});

btnSaveConfig.addEventListener('click', saveConfig);

// ============================================================================
// SKILLS SYSTEM
// ============================================================================

let allSkills = [];

async function loadSkills() {
    try {
        const res = await fetch('/api/skills');
        const data = await res.json();
        if (data.success) {
            allSkills = data.data;
            renderSkillsList();
            renderSkillsSidebar();
        }
    } catch (e) {
        console.error('Load skills error:', e);
    }
}

function renderSkillsList() {
    const container = document.getElementById('skills-list');
    if (!container) return;
    if (allSkills.length === 0) {
        container.innerHTML = '<p style="color:#666;text-align:center;padding:20px">Nenhuma skill criada ainda.</p>';
        return;
    }
    container.innerHTML = '';
    for (const skill of allSkills) {
        const el = document.createElement('div');
        el.style.cssText = 'display:flex;align-items:center;justify-content:space-between;padding:10px 12px;border:1px solid #333;border-radius:6px;margin-bottom:6px;background:#0f0f1e';
        el.innerHTML = `
            <div style="flex:1">
                <span style="color:#a78bfa;font-weight:600;font-size:13px">${escapeHtml(skill.name)}</span>
                <span style="color:#666;font-size:11px;margin-left:8px">${escapeHtml(skill.category)}</span>
                <div style="color:#888;font-size:11px;margin-top:2px">${escapeHtml(skill.description)}</div>
            </div>
            <div style="display:flex;gap:6px;align-items:center">
                <label style="display:flex;align-items:center;gap:4px;cursor:pointer;font-size:11px;color:#888">
                    <input type="checkbox" ${skill.enabled ? 'checked' : ''} onchange="toggleSkill('${skill.id}', this.checked)" style="accent-color:#7c3aed">
                    Ativo
                </label>
                <button onclick="editSkill('${skill.id}')" style="background:#333;color:#fff;border:none;padding:4px 8px;border-radius:4px;cursor:pointer;font-size:11px">Editar</button>
                <button onclick="deleteSkill('${skill.id}')" style="background:#dc2626;color:#fff;border:none;padding:4px 8px;border-radius:4px;cursor:pointer;font-size:11px">Excluir</button>
            </div>
        `;
        container.appendChild(el);
    }
}

async function toggleSkill(id, enabled) {
    try {
        const current = state.activeConversation || {};
        const selected = new Set(Array.isArray(current.skillIds) ? current.skillIds : allSkills.filter(skill => skill.enabled).map(skill => skill.id));
        if (enabled) selected.add(id); else selected.delete(id);
        const skillIds = [...selected];
        if (state.currentConversationId) await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ skillIds }) });
        if (state.activeConversation) state.activeConversation.skillIds = skillIds;
        renderSkillsSidebar();
    } catch (e) {
        console.error('Toggle skill error:', e);
    }
}

function editSkill(id) {
    const skill = allSkills.find(s => s.id === id);
    if (!skill) return;
    document.getElementById('skill-editor').classList.remove('hidden');
    document.getElementById('skill-editor-title').textContent = 'Editar Skill';
    document.getElementById('skill-edit-id').value = skill.id;
    document.getElementById('skill-name').value = skill.name;
    document.getElementById('skill-category').value = skill.category;
    document.getElementById('skill-description').value = skill.description;
    document.getElementById('skill-prompt').value = skill.prompt;
}

async function deleteSkill(id) {
    if (!confirm('Excluir esta skill?')) return;
    try {
        await fetch('/api/skills/' + id, { method: 'DELETE' });
        allSkills = allSkills.filter(s => s.id !== id);
        renderSkillsList();
    } catch (e) {
        console.error('Delete skill error:', e);
    }
}

async function saveSkill() {
    const id = document.getElementById('skill-edit-id').value;
    const name = document.getElementById('skill-name').value.trim();
    const category = document.getElementById('skill-category').value.trim();
    const description = document.getElementById('skill-description').value.trim();
    const prompt = document.getElementById('skill-prompt').value.trim();

    if (!name) return alert('Nome obrigatorio.');

    const body = { id: id || name, name, category, description, prompt, enabled: true };

    try {
        const method = id ? 'PUT' : 'POST';
        const url = id ? '/api/skills/' + id : '/api/skills';
        const res = await fetch(url, {
            method,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body)
        });
        const data = await res.json();
        if (data.success) {
            document.getElementById('skill-editor').classList.add('hidden');
            document.getElementById('skill-name').value = '';
            document.getElementById('skill-category').value = '';
            document.getElementById('skill-description').value = '';
            document.getElementById('skill-prompt').value = '';
            document.getElementById('skill-edit-id').value = '';
            await loadSkills();
        }
    } catch (e) {
        console.error('Save skill error:', e);
    }
}

document.addEventListener('click', async (e) => {
    if (e.target.id === 'btn-new-skill') {
        document.getElementById('skill-editor').classList.remove('hidden');
        document.getElementById('skill-editor-title').textContent = 'Nova Skill';
        document.getElementById('skill-edit-id').value = '';
        document.getElementById('skill-name').value = '';
        document.getElementById('skill-category').value = '';
        document.getElementById('skill-description').value = '';
        document.getElementById('skill-prompt').value = '';
    }
    if (e.target.id === 'btn-skill-cancel') {
        document.getElementById('skill-editor').classList.add('hidden');
    }
    if (e.target.id === 'btn-skill-save') {
        saveSkill();
    }
    if (e.target.id === 'btn-manage-skills') {
        btnConfig.click();
        setTimeout(() => {
            document.querySelectorAll('.config-tab').forEach(t => { t.style.color = '#666'; t.style.borderBottom = 'none'; });
            const skillsTab = document.querySelector('[data-tab="skills"]');
            if (skillsTab) { skillsTab.style.color = '#a78bfa'; skillsTab.style.borderBottom = '2px solid #a78bfa'; }
            document.querySelectorAll('.config-tab-content').forEach(c => c.classList.add('hidden'));
            document.getElementById('tab-skills').classList.remove('hidden');
            loadSkills();
        }, 100);
    }
    if (e.target.id === 'btn-export-skills') {
        exportSkills();
    }
    if (e.target.id === 'btn-import-skills') {
        document.getElementById('skill-file-input').click();
    }
    if (e.target.id === 'btn-import-folder') {
        document.getElementById('folder-import-panel').classList.toggle('hidden');
    }
    if (e.target.id === 'btn-cancel-folder-import') {
        document.getElementById('folder-import-panel').classList.add('hidden');
        document.getElementById('folder-scan-result').innerHTML = '';
        document.getElementById('btn-confirm-folder-import').classList.add('hidden');
        selectedFolderHandle = null;
    }
    if (e.target.id === 'btn-select-folder') {
        await selectFolder();
    }
    if (e.target.id === 'btn-confirm-folder-import') {
        await importSelectedFolder();
    }
    if (e.target.classList.contains('config-tab')) {
        document.querySelectorAll('.config-tab').forEach(t => { t.style.color = '#666'; t.style.borderBottom = 'none'; });
        e.target.style.color = '#a78bfa';
        e.target.style.borderBottom = '2px solid #a78bfa';
        document.querySelectorAll('.config-tab-content').forEach(c => c.classList.add('hidden'));
        document.getElementById('tab-' + e.target.dataset.tab).classList.remove('hidden');
        if (e.target.dataset.tab === 'skills') loadSkills();
    }

    const skillItem = e.target.closest('.skill-toggle-item');
    if (skillItem) {
        const id = skillItem.dataset.id;
        const skill = allSkills.find(s => s.id === id);
        if (skill) {
            const selected = state.activeConversation && Array.isArray(state.activeConversation.skillIds)
                ? state.activeConversation.skillIds.includes(id)
                : skill.enabled;
            toggleSkill(id, !selected);
        }
    }
});

function renderSkillsSidebar() {
    const container = document.getElementById('skills-sidebar-list');
    if (!container) return;
    if (allSkills.length === 0) {
        container.innerHTML = '<div style="color:#555;font-size:11px;text-align:center;padding:8px">Nenhuma skill</div>';
        return;
    }
    container.innerHTML = '';
    for (const skill of allSkills) {
        const el = document.createElement('div');
        const selected = state.activeConversation && Array.isArray(state.activeConversation.skillIds) ? state.activeConversation.skillIds.includes(skill.id) : skill.enabled;
        el.className = 'skill-toggle-item' + (selected ? ' active' : '');
        el.dataset.id = skill.id;
        el.innerHTML = `<span class="skill-dot"></span><span class="skill-label">${escapeHtml(skill.name)}</span>`;
        container.appendChild(el);
    }
}

function exportSkills() {
    if (allSkills.length === 0) return alert('Nenhuma skill para exportar.');
    const data = JSON.stringify(allSkills.map(s => ({ name: s.name, category: s.category, description: s.description, prompt: s.prompt, enabled: s.enabled })), null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'nexa-skills-' + new Date().toISOString().slice(0, 10) + '.json';
    a.click();
    URL.revokeObjectURL(url);
}

async function importSkills(file) {
    try {
        const text = await file.text();
        const skills = JSON.parse(text);
        if (!Array.isArray(skills)) return alert('Formato invalido. Esperado um array de skills.');
        let count = 0;
        for (const s of skills) {
            if (!s.name) continue;
            const id = s.name.toLowerCase().replace(/[^a-z0-9_-]/g, '-').substring(0, 40);
            await fetch('/api/skills', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ id, name: s.name, category: s.category || 'Importada', description: s.description || '', prompt: s.prompt || '', enabled: s.enabled !== false })
            });
            count++;
        }
        await loadSkills();
        alert(count + ' skills importadas com sucesso!');
    } catch (e) {
        alert('Erro ao importar: ' + e.message);
    }
}

let selectedFolderHandle = null;

async function selectFolder() {
    if (!window.showDirectoryPicker) {
        alert('Seletor de pasta nao suportado neste navegador. Use Chrome ou Edge.');
        return;
    }
    try {
        selectedFolderHandle = await window.showDirectoryPicker();
        const resultDiv = document.getElementById('folder-scan-result');
        resultDiv.innerHTML = '<div style="color:#888;font-size:12px">Lendo pasta...</div>';

        const items = [];
        for await (const entry of selectedFolderHandle.values()) {
            if (entry.kind === 'file' && (entry.name.endsWith('.md') || entry.name.endsWith('.txt'))) {
                items.push({ name: entry.name, type: 'file' });
            }
            if (entry.kind === 'directory') {
                let subCount = 0;
                for await (const sub of entry.values()) {
                    if (sub.kind === 'file' && (sub.name.endsWith('.md') || sub.name.endsWith('.txt'))) subCount++;
                }
                if (subCount > 0) items.push({ name: entry.name, type: 'folder', fileCount: subCount });
            }
        }

        if (items.length === 0) {
            resultDiv.innerHTML = '<div style="color:#888;font-size:12px">Nenhum arquivo .md encontrado em: ' + escapeHtml(selectedFolderHandle.name) + '</div>';
            return;
        }

        resultDiv.innerHTML = '<div style="color:#22c55e;font-size:12px;margin-bottom:6px">&#x2713; ' + selectedFolderHandle.name + ' — ' + items.length + ' itens encontrados:</div>';
        for (const item of items) {
            const el = document.createElement('div');
            el.style.cssText = 'padding:4px 8px;font-size:12px;color:#ccc';
            if (item.type === 'folder') {
                el.innerHTML = '&#x1F4C1; ' + escapeHtml(item.name) + ' <span style="color:#666">(' + item.fileCount + ' .md)</span>';
            } else {
                el.innerHTML = '&#x1F4C4; ' + escapeHtml(item.name);
            }
            resultDiv.appendChild(el);
        }
        document.getElementById('btn-confirm-folder-import').classList.remove('hidden');
    } catch (e) {
        if (e.name !== 'AbortError') {
            document.getElementById('folder-scan-result').innerHTML = '<div style="color:#ef4444;font-size:12px">Erro: ' + e.message + '</div>';
        }
    }
}

async function importSelectedFolder() {
    if (!selectedFolderHandle) return;
    const resultDiv = document.getElementById('folder-scan-result');
    resultDiv.innerHTML = '<div style="color:#888;font-size:12px">Importando...</div>';

    let count = 0;
    const errors = [];

    for await (const entry of selectedFolderHandle.values()) {
        if (entry.kind === 'file' && (entry.name.endsWith('.md') || entry.name.endsWith('.txt'))) {
            try {
                const file = await entry.getFile();
                const content = await file.text();
                const name = entry.name.replace(/\.(md|txt)$/i, '');
                const id = name.toLowerCase().replace(/[^a-z0-9_-]/g, '-').substring(0, 50);

                let description = '';
                const lines = content.split('\n');
                for (const line of lines) {
                    if (line.startsWith('# ')) {
                        description = line.replace(/^#+\s*/, '').trim();
                        break;
                    }
                }

                await fetch('/api/skills', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        id,
                        name,
                        category: selectedFolderHandle.name,
                        description: description || name,
                        prompt: content,
                        enabled: true
                    })
                });
                count++;
            } catch (e) {
                errors.push(entry.name + ': ' + e.message);
            }
        }

        if (entry.kind === 'directory') {
            const dirName = entry.name;
            for await (const sub of entry.values()) {
                if (sub.kind === 'file' && (sub.name.endsWith('.md') || sub.name.endsWith('.txt'))) {
                    try {
                        const file = await sub.getFile();
                        const content = await file.text();
                        const name = sub.name.replace(/\.(md|txt)$/i, '');
                        const id = (dirName + '-' + name).toLowerCase().replace(/[^a-z0-9_-]/g, '-').substring(0, 50);

                        let description = '';
                        const lines = content.split('\n');
                        for (const line of lines) {
                            if (line.startsWith('# ')) {
                                description = line.replace(/^#+\s*/, '').trim();
                                break;
                            }
                        }

                        await fetch('/api/skills', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({
                                id,
                                name,
                                category: dirName,
                                description: description || name,
                                prompt: content,
                                enabled: true
                            })
                        });
                        count++;
                    } catch (e) {
                        errors.push(sub.name + ': ' + e.message);
                    }
                }
            }
        }
    }

    await loadSkills();
    let msg = count + ' skills importadas de ' + selectedFolderHandle.name;
    if (errors.length > 0) msg += '\n\nErros: ' + errors.join(', ');
    resultDiv.innerHTML = '<div style="color:#22c55e;font-size:12px">&#x2713; ' + msg + '</div>';
    document.getElementById('btn-confirm-folder-import').classList.add('hidden');
    selectedFolderHandle = null;
}

// ============================================================================
// UTILITIES
// ============================================================================

// ============================================================================
// CONVERSATIONS (histórico persistente + memória)
// ============================================================================

// Busca a lista de conversas salvas no servidor e renderiza no sidebar
async function loadConversations() {
    try {
        const response = await fetch('/api/conversations');
        const data = await response.json();
        if (data.success) {
            state.conversations = data.data || [];
            renderConversationList();
            renderWorkspaceList();
        }
    } catch (error) {
        console.warn('Erro ao carregar conversas:', error.message);
    }
}

async function loadWorkspaces() {
    try {
        const response = await fetch('/api/workspaces');
        const data = await response.json();
        if (data.success) {
            state.workspaces = data.data || [];
            renderWorkspaceList();
        }
    } catch (error) {
        console.warn('Erro ao carregar projetos:', error.message);
    }
}

function renderWorkspaceList() {
    if (!workspaceList) return;
    workspaceList.innerHTML = '';
    if (!state.workspaces.length) {
        workspaceList.innerHTML = '<div class="conv-empty">Abra uma pasta para criar seu primeiro projeto.</div>';
        return;
    }
    for (const workspace of state.workspaces) {
        const linked = state.conversations.filter(conversation => conversation.workspaceId === workspace.id);
        const item = document.createElement('button');
        item.type = 'button';
        item.className = 'conv-item workspace-item';
        item.innerHTML = '<div class="conv-item-main"><div class="conv-item-title">📁 ' + escapeHtml(workspace.name) + '</div><div class="conv-item-meta">' + linked.length + ' conversa(s)</div></div>';
        item.addEventListener('click', async () => {
            if (linked.length) return openConversation(linked[0].id);
            const id = await createNewConversationOnServer(workspace.projectPath, workspace.name);
            if (id) await openConversation(id);
        });
        workspaceList.appendChild(item);
    }
}

function renderConversationList() {
    convList.innerHTML = '';
    if (state.conversations.length === 0) {
        convList.innerHTML = '<div class="conv-empty">Nenhuma conversa salva ainda.</div>';
        return;
    }

    const groups = {};
    for (const conv of state.conversations) {
        const key = conv.projectName || '_general';
        if (!groups[key]) groups[key] = [];
        groups[key].push(conv);
    }

    const order = Object.keys(groups).sort((a, b) => {
        if (a === '_general') return 1;
        if (b === '_general') return -1;
        return a.localeCompare(b);
    });

    for (const groupName of order) {
        if (groupName !== '_general') {
            const header = document.createElement('div');
            header.className = 'conv-list-header';
            header.innerHTML = '<span class="tree-icon">&#x1F4C1;</span> ' + escapeHtml(groupName);
            convList.appendChild(header);
        }

        for (const conv of groups[groupName]) {
            const item = document.createElement('div');
            item.className = 'conv-item';
            if (conv.id === state.currentConversationId) item.classList.add('active');

            const title = conv.title && conv.title !== 'Nova conversa' ? conv.title : 'Sem título';
            const meta = [];
            if (conv.messageCount) meta.push(`${conv.messageCount} msg`);
            if (conv.hasMemory) meta.push('🧠');

            item.innerHTML = `
                <div class="conv-item-main">
                    <div class="conv-item-title">${escapeHtml(title)}</div>
                    <div class="conv-item-meta">${meta.join(' · ')}</div>
                </div>
                <button class="conv-del" data-id="${conv.id}" title="Excluir">🗑</button>
            `;

        // Clique abre a conversa
        item.addEventListener('click', (e) => {
            if (e.target.classList.contains('conv-del')) return;
            openConversation(conv.id);
        });

        // Excluir
        const delBtn = item.querySelector('.conv-del');
        delBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            deleteConversation(conv.id);
        });

        convList.appendChild(item);
        }
    }
}

// Nomeia automaticamente e salva a conversa (chamado na primeira troca)
let convTitleTimeout = null;
function scheduleAutoTitle() {
    clearTimeout(convTitleTimeout);
    convTitleTimeout = setTimeout(() => {
        if (state.currentConversationId && state.messages.length > 0) {
            const first = state.messages.find(m => m.role === 'user');
            if (first) {
                const base = first.content.slice(0, 50);
                renameConversation(state.currentConversationId, base);
            }
        }
    }, 1500);
}

async function createNewConversationOnServer(projectPath, projectName) {
    try {
        const response = await fetch('/api/conversations', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ projectPath, projectName })
        });
        const data = await response.json();
        if (data.success) {
            state.currentConversationId = data.data.id;
            localStorage.setItem('nexa_current_conversation', data.data.id);
            if (languageInput) languageInput.value = '';
            if (projectPath) localStorage.setItem('nexa_last_project', projectPath);
        }
        await loadConversations();
        return state.currentConversationId;
    } catch (error) {
        console.error('Erro ao criar conversa:', error);
        return null;
    }
}

async function persistActiveConversation(id) {
    if (!id) return;
    try {
        await fetch('/api/session/active', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ conversationId: id }) });
    } catch (error) { console.warn('Não foi possível persistir a conversa ativa:', error.message); }
}

async function chooseProjectDirectory() {
    if (window.nexaSecrets && typeof window.nexaSecrets.chooseProjectDirectory === 'function') {
        return await window.nexaSecrets.chooseProjectDirectory();
    }
    return new Promise((resolve) => {
        const overlay = document.createElement('div');
        overlay.className = 'create-project-modal';
        overlay.innerHTML = `
            <div class="create-project-box" style="width:480px">
                <h3>Selecionar pasta do projeto</h3>
                <p style="color:#999;font-size:13px;margin-bottom:12px">Navegue até a pasta do projeto e clique em Selecionar</p>
                <div id="folder-browser" style="max-height:300px;overflow-y:auto;background:#0f0f1e;border:1px solid #333;border-radius:6px;padding:4px;margin-bottom:12px"></div>
                <div class="create-actions">
                    <button class="btn-close-viewer" id="folder-cancel">Cancelar</button>
                    <button class="btn-analyze" id="folder-ok" disabled>Selecionar</button>
                </div>
            </div>
        `;
        document.body.appendChild(overlay);

        const list = overlay.querySelector('#folder-browser');
        let selectedPath = null;

        async function loadFolders(path) {
            try {
                const url = path ? `/api/projects/browse?path=${encodeURIComponent(path)}` : '/api/projects/browse';
                const res = await fetch(url);
                const data = await res.json();
                if (data.success && data.data.folders) {
                    list.innerHTML = '';
                    if (path) {
                        const back = document.createElement('div');
                        back.className = 'tree-item';
                        back.innerHTML = '<span class="tree-icon">..</span><span class="tree-name">Voltar</span>';
                        back.addEventListener('click', () => {
                            const parts = path.split(/[\\/]/);
                            parts.pop();
                            const parent = parts.join('/') || parts[0] + '/';
                            loadFolders(parent);
                        });
                        list.appendChild(back);
                    }
                    for (const folder of data.data.folders) {
                        const item = document.createElement('div');
                        item.className = 'tree-item';
                        item.dataset.path = folder.path;
                        item.innerHTML = `<span class="tree-icon">&#x1F4C1;</span><span class="tree-name">${folder.name}</span>`;
                        item.addEventListener('click', () => {
                            list.querySelectorAll('.tree-item').forEach(el => el.style.background = '');
                            item.style.background = 'rgba(124,58,237,0.2)';
                            selectedPath = folder.path;
                            overlay.querySelector('#folder-ok').disabled = false;
                        });
                        item.addEventListener('dblclick', () => loadFolders(folder.path));
                        list.appendChild(item);
                    }
                    if (data.data.folders.length === 0) {
                        list.innerHTML = '<div style="color:#666;padding:12px;text-align:center">Nenhuma subpasta encontrada</div>';
                    }
                }
            } catch (err) {
                list.innerHTML = '<div style="color:#ef4444;padding:12px">Erro ao carregar pastas</div>';
            }
        }

        loadFolders(null);

        overlay.querySelector('#folder-ok').addEventListener('click', () => {
            overlay.remove();
            resolve(selectedPath);
        });
        overlay.querySelector('#folder-cancel').addEventListener('click', () => {
            overlay.remove();
            resolve(null);
        });
    });
}

async function attachProjectToCurrentConversation() {
    const projectPath = await chooseProjectDirectory();
    if (!projectPath) return;
    try {
        const validation = await fetch('/api/projects/load', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: projectPath })
        }).then(response => response.json());
        if (!validation.success) throw new Error(validation.error || 'Não foi possível abrir o projeto.');

        if (!state.currentConversationId) await createNewConversationOnServer();
        const project = validation.data;
        const response = await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ projectPath: project.path, projectName: project.name })
        });
        const data = await response.json();
        if (!data.success) throw new Error(data.error || 'Não foi possível vincular o projeto.');

        state.project = project;
        state.activeConversation = data.data;
        localStorage.setItem('nexa_last_project', project.path);
        showProjectInSidebar(project.path, project.name);
        await loadConversations();
        renderConversationList();
        userInput.focus();
    } catch (error) {
        console.error('Erro ao vincular projeto:', error);
        alert(error.message || 'Não foi possível vincular este projeto à conversa.');
    }
}

// "Nova conversa": cria uma conversa vazia no servidor e limpa a tela.
// As conversas antigas NÃO são perdidas — ficam salvas no histórico.
async function newChat() {
    state.messages = [];
    state.attachments = [];
    renderAttachmentBar();
    memoryPanel.classList.add('hidden');
    memoryText.value = '';

    const overlay = document.createElement('div');
    overlay.className = 'create-project-modal';
    overlay.innerHTML = `
        <div class="create-project-box" style="width:420px">
            <h3>Nova Conversa</h3>
            <p style="color:#999;font-size:13px;margin-bottom:12px">Vincular a um projeto? (opcional)</p>
            <div id="chat-project-list" style="max-height:200px;overflow-y:auto;background:#0f0f1e;border:1px solid #333;border-radius:6px;padding:4px;margin-bottom:12px"></div>
            <div class="create-actions">
                <button class="btn-close-viewer" id="chat-proj-cancel">Cancelar</button>
                <button class="btn-analyze" id="chat-proj-ok">Iniciar</button>
            </div>
        </div>
    `;
    document.body.appendChild(overlay);

    const list = overlay.querySelector('#chat-project-list');
    let selectedPath = null;
    let selectedName = null;

    const noProj = document.createElement('div');
    noProj.className = 'tree-item';
    noProj.style.cssText = 'color:#7c3aed;font-weight:bold';
    noProj.innerHTML = '<span class="tree-icon">&#x1F4AC;</span><span class="tree-name">Conversa geral (sem projeto)</span>';
    noProj.addEventListener('click', () => {
        selectedPath = null;
        selectedName = null;
        list.querySelectorAll('.tree-item').forEach(el => el.style.background = '');
        noProj.style.background = 'rgba(124,58,237,0.2)';
    });
    list.appendChild(noProj);

    for (const workspace of state.workspaces) {
        if (!workspace.projectPath) continue;
        const name = workspace.name || workspace.projectPath.split(/[\\/]/).pop();
        const projItem = document.createElement('div');
        projItem.className = 'tree-item';
        projItem.innerHTML = '<span class="tree-icon">&#x1F4C1;</span><span class="tree-name">' + name + '</span>';
        projItem.addEventListener('click', () => {
            selectedPath = workspace.projectPath;
            selectedName = name;
            list.querySelectorAll('.tree-item').forEach(el => el.style.background = '');
            projItem.style.background = 'rgba(124,58,237,0.2)';
        });
        list.appendChild(projItem);
        if (!selectedPath) {
            selectedPath = workspace.projectPath;
            selectedName = name;
            projItem.style.background = 'rgba(124,58,237,0.2)';
        }
    }

    try {
        const res = await fetch('/api/projects/browse');
        const data = await res.json();
        if (data.success && data.data.folders) {
            for (const drive of data.data.folders) {
                const dItem = document.createElement('div');
                dItem.className = 'tree-item';
                dItem.innerHTML = '<span class="tree-icon">&#x1F4BB;</span><span class="tree-name">' + drive.name + '</span>';
                dItem.addEventListener('click', async () => {
                    const subRes = await fetch('/api/projects/browse?path=' + encodeURIComponent(drive.path));
                    const subData = await subRes.json();
                    if (subData.success) {
                        for (const f of subData.data.folders) {
                            const exist = list.querySelector('[data-path="' + f.path.replace(/\\/g, '\\\\') + '"]');
                            if (!exist) {
                                const item = document.createElement('div');
                                item.className = 'tree-item';
                                item.dataset.path = f.path;
                                item.innerHTML = '<span class="tree-icon">&#x1F4C1;</span><span class="tree-name">' + f.name + '</span>';
                                item.addEventListener('click', () => {
                                    selectedPath = f.path;
                                    selectedName = f.name;
                                    list.querySelectorAll('.tree-item').forEach(el => el.style.background = '');
                                    item.style.background = 'rgba(124,58,237,0.2)';
                                });
                                list.appendChild(item);
                            }
                        }
                    }
                });
                list.appendChild(dItem);
            }
        }
    } catch (e) {}

    overlay.querySelector('#chat-proj-cancel').addEventListener('click', () => overlay.remove());
    overlay.querySelector('#chat-proj-ok').addEventListener('click', async () => {
        overlay.remove();
        const id = await createNewConversationOnServer(selectedPath, selectedName);
        if (!id) {
            state.currentConversationId = null;
        }
        showProjectInSidebar(selectedPath, selectedName);
        let welcomeMsg = 'Como posso ajudar você hoje?';
        if (selectedName) welcomeMsg = 'Projeto **' + selectedName + '** vinculado. Como posso ajudar?';
        messagesContainer.innerHTML = `
            <div class="welcome">
                <div class="welcome-logo">🤖</div>
                <h2>NEXA AI Assistant</h2>
                <p>${welcomeMsg}</p>
            </div>`;
    });
}

// Abre uma conversa salva e carrega seu histórico + memória
async function openConversation(id) {
    try {
        const response = await fetch(`/api/conversations/${encodeURIComponent(id)}`);
        const data = await response.json();
        if (!data.success || !data.data) return;

        const conv = data.data;
        state.activeConversation = conv;
        state.currentConversationId = conv.id;
        localStorage.setItem('nexa_current_conversation', conv.id);
        await persistActiveConversation(conv.id);
        if (conv.projectPath) localStorage.setItem('nexa_last_project', conv.projectPath);
        if (languageInput) languageInput.value = conv.language || '';
        state.messages = conv.messages.map(m => ({
            role: m.role === 'assistant' ? 'ai' : 'user',
            content: m.content,
            images: m.images || []
        }));

        // Limpa a tela
        messagesContainer.innerHTML = '';
        const welcomeEl = messagesContainer.querySelector('.welcome');
        if (welcomeEl) welcomeEl.remove();

        // Renderiza o histórico salvo
        for (const m of conv.messages) {
            if (m.role === 'assistant' && isUnsafeHistoricalAssistantOutput(m.content)) continue;
            const role = m.role === 'assistant' ? 'ai' : 'user';
            addMessage(role, m.content, m.images || []);
            if (m.role === 'assistant' && Array.isArray(m.actionResults) && m.actionResults.length > 0) {
                renderInlineCodeDiff(m.actionResults);
            }
        }
        if (conv.projectPath) {
            const assistant = [...conv.messages].reverse().find(m => m.role === 'assistant'
                && Array.isArray(m.actionResults) && m.actionResults.some(r => r.ok
                    && ['write_file', 'create_file', 'replace_text', 'create_project'].includes(r.kind)));
            if (assistant) {
                const lastMessage = document.querySelector('#messages .message:last-child');
                const wrapper = document.createElement('div');
                wrapper.style.display = 'contents';
                const btn = document.createElement('button');
                btn.type = 'button';
                btn.className = 'next-step-button';
                btn.textContent = '➤ PRÓXIMA ETAPA';
                btn.title = 'Continuar construindo este projeto (envia "prossiga")';
                btn.addEventListener('click', () => {
                    btn.disabled = true;
                    btn.textContent = '▶ Construindo próximo passo…';
                    sendPrompt('prossiga', []).catch(() => {
                        btn.disabled = false;
                        btn.textContent = '➤ PRÓXIMA ETAPA';
                    });
                });
                if (lastMessage) {
                    lastMessage.appendChild(btn);
                    lastMessage.scrollIntoView({ block: 'nearest' });
                } else {
                    messagesContainer.appendChild(btn);
                }
            }
        }

        // Carrega a memória de longo prazo, se houver
        memoryPanel.classList.add('hidden');
        if (conv.memory && conv.memory.trim()) {
            memoryText.value = conv.memory;
        } else {
            memoryText.value = '';
        }

        showProjectInSidebar(conv.projectPath, conv.projectName);
        state.provider = conv.providerPreference || (state.config && state.config.provider) || 'local';
        providerSelect.value = state.provider;
        if (conv.modelPreference) modelSelect.value = conv.modelPreference;
        await fetchModels();
        await checkAIStatus();
        renderSkillsSidebar();
        renderConversationList();
    } catch (error) {
        console.error('Erro ao abrir conversa:', error);
    }
}

async function renameConversation(id, title) {
    try {
        await fetch(`/api/conversations/${encodeURIComponent(id)}`, {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ title })
        });
        await loadConversations();
    } catch (error) {
        console.warn('Erro ao renomear conversa:', error.message);
    }
}

async function deleteConversation(id) {
    if (!confirm('Excluir esta conversa permanentemente?')) return;
    try {
        const response = await fetch(`/api/conversations/${encodeURIComponent(id)}`, { method: 'DELETE' });
        const data = await response.json();
        if (!data.success) throw new Error(data.error || 'Não foi possível excluir a conversa.');
        // Se era a conversa ativa, abre uma nova
        if (state.currentConversationId === id) {
            state.currentConversationId = null;
            await newChat();
        } else {
            await loadConversations();
        }
        if (data.removedWorkspaceId) {
            state.workspaces = state.workspaces.filter(workspace => workspace.id !== data.removedWorkspaceId);
            renderWorkspaceList();
        }
    } catch (error) {
        console.error('Erro ao excluir conversa:', error.message);
    }
}

// ============================================================================
// MEMÓRIA DE LONGO PRAZO
// ============================================================================

btnOpenMemory.addEventListener('click', (e) => {
    e.preventDefault();
    memoryPanel.classList.toggle('hidden');
});

btnCloseMemory.addEventListener('click', () => {
    memoryPanel.classList.add('hidden');
});

btnSaveMemory.addEventListener('click', async () => {
    if (!state.currentConversationId) {
        await createNewConversationOnServer();
    }
    const memory = memoryText.value.trim();
    try {
        const response = await fetch(`/api/conversations/${encodeURIComponent(state.currentConversationId)}/memory`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ memory })
        });
        if (response.ok) {
            btnSaveMemory.textContent = '✓ Salvo';
            setTimeout(() => btnSaveMemory.textContent = '💾 Salvar', 2000);
            await loadConversations();
        }
    } catch (error) {
        console.error('Erro ao salvar memória:', error.message);
    }
});

// ============================================================================
// ANEXOS (imagens / arquivos)
// ============================================================================

fileInput.addEventListener('change', (e) => {
    const files = Array.from(e.target.files || []);
    for (const file of files) {
        const isImage = file.type.startsWith('image/');
        const reader = new FileReader();
        reader.onload = (ev) => {
            state.attachments.push({
                name: file.name,
                isImage,
                dataUrl: ev.target.result
            });
            renderAttachmentBar();
        };
        reader.readAsDataURL(file);
    }
    fileInput.value = '';
});

const skillFileInput = document.getElementById('skill-file-input');
if (skillFileInput) {
    skillFileInput.addEventListener('change', (e) => {
        const file = e.target.files[0];
        if (file) importSkills(file);
        skillFileInput.value = '';
    });
}

function renderAttachmentBar() {
    attachmentsEl.innerHTML = '';
    if (state.attachments.length === 0) {
        attachmentsEl.classList.add('hidden');
        return;
    }
    attachmentsEl.classList.remove('hidden');
    for (let i = 0; i < state.attachments.length; i++) {
        const a = state.attachments[i];
        const chip = document.createElement('div');
        chip.className = 'attach-chip';
        chip.innerHTML = `
            ${a.isImage ? '<img class="attach-thumb" src="' + a.dataUrl + '" alt="">' : '📄'}
            <span class="attach-name">${escapeHtml(a.name)}</span>
            <button class="attach-remove" data-i="${i}">✕</button>
        `;
        chip.querySelector('.attach-remove').addEventListener('click', () => {
            state.attachments.splice(i, 1);
            renderAttachmentBar();
        });
        attachmentsEl.appendChild(chip);
    }
}

// ============================================================================
// APP STARTUP
// ============================================================================

document.addEventListener('DOMContentLoaded', init);
﻿
﻿
// ========== MODEL STORE MODAL ==========
let storeModels = [];
let storeFilter = 'all';
let downloadPolling = null;

function initModelStore() {
    const searchBtn = document.getElementById('store-search-btn');
    const searchInput = document.getElementById('store-search-input');
    const sortSelect = document.getElementById('store-sort');
    const closeBtn = document.getElementById('store-close-btn');
    const overlay = document.getElementById('model-store-modal');
    const onboardingModel = document.getElementById('btn-onboarding-model');
    const onboardingProvider = document.getElementById('btn-onboarding-provider');
    const storeGrid = document.getElementById('store-grid');
    
    if (searchBtn) searchBtn.addEventListener('click', searchModels);
    if (searchInput) searchInput.addEventListener('keydown', e => { if (e.key === 'Enter') searchModels(); });
    if (sortSelect) sortSelect.addEventListener('change', searchModels);
    if (closeBtn) closeBtn.addEventListener('click', closeModelStore);
    if (overlay) overlay.addEventListener('click', e => { if (e.target === overlay) closeModelStore(); });
    if (onboardingModel) onboardingModel.addEventListener('click', openModelStore);
    if (onboardingProvider) onboardingProvider.addEventListener('click', () => {
        cfgProvider.value = 'openai';
        syncConfigSections();
        configModal.classList.remove('hidden');
        cfgOpenaiKey.focus();
    });
    if (storeGrid) storeGrid.addEventListener('click', event => {
        const downloadButton = event.target.closest('[data-download-model]');
        if (downloadButton) return downloadModel(downloadButton.dataset.downloadModel, downloadButton.dataset.downloadFile);
        const variantsButton = event.target.closest('[data-toggle-variants]');
        if (variantsButton) toggleVariants(variantsButton);
    });
    document.addEventListener('keydown', e => { if (e.key === 'Escape') closeModelStore(); });
    
    document.querySelectorAll('.filter-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            storeFilter = btn.dataset.filter;
            renderStoreModels();
        });
    });
    
    const sidebar = document.querySelector('.sidebar-content') || document.querySelector('.sidebar') || document.querySelector('aside');
    if (sidebar) {
        const storeBtn = document.createElement('button');
        storeBtn.innerHTML = 'Model Store';
        storeBtn.style.cssText = 'width:100%;padding:10px;background:linear-gradient(135deg,#7c3aed,#2563eb);color:white;border:none;border-radius:8px;cursor:pointer;font-size:14px;margin:8px 0;text-align:left;';
        storeBtn.addEventListener('click', openModelStore);
        sidebar.insertBefore(storeBtn, sidebar.firstChild);
    }
    
    startDownloadPolling();
}

function openModelStore() {
    const modal = document.getElementById('model-store-modal');
    if (modal) {
        modal.style.display = 'flex';
        modal.style.cssText = 'position:fixed;top:0;left:0;width:100vw;height:100vh;background:rgba(0,0,0,0.6);display:flex;align-items:center;justify-content:center;z-index:10000;';
        if (storeModels.length === 0) searchModels();
    }
}

function closeModelStore() {
    const modal = document.getElementById('model-store-modal');
    if (modal) modal.style.display = 'none';
}

async function searchModels() {
    const query = document.getElementById('store-search-input')?.value || '';
    const sort = document.getElementById('store-sort')?.value || 'downloads';
    const grid = document.getElementById('store-grid');
    const loading = document.getElementById('store-loading');
    if (loading) loading.style.display = 'block';
    if (grid) grid.innerHTML = '';
    try {
        let task = '';
        if (storeFilter === 'code') task = 'text-generation';
        if (storeFilter === 'vision') task = 'image-to-text';
        const url = '/api/models/store?q=' + encodeURIComponent(query) + '&sort=' + sort + '&limit=24' + (task ? '&task=' + task : '');
        const res = await fetch(url);
        const data = await res.json();
        if (data.success) { storeModels = data.data; renderStoreModels(); }
    } catch(e) { console.error('Erro:', e); }
    if (loading) loading.style.display = 'none';
}

﻿function renderStoreModels() {
    const grid = document.getElementById('store-grid');
    if (!grid) return;
    let filtered = storeModels;
    if (storeFilter === 'vision') filtered = storeModels.filter(m => m.hasVision);
    grid.innerHTML = filtered.map(m => {
        const tags = m.hasVision ? '<span class="store-tag vision">Visao</span>' : '';
        const quants = (m.quantizations || []).map(q => '<span class="store-tag">' + escapeHtml(String(q)) + '</span>').join('');
        const sizeTag = m.totalSizeGB !== '?' ? '<span class="store-tag">' + escapeHtml(String(m.totalSizeGB)) + ' GB</span>' : '';
        const initialVars = m.variants.slice(0, 4).map(v => {
            const name = escapeHtml(String(v.filename).split('/').pop());
            return '<div class="store-variant"><span class="store-variant-name">' + name + '</span><span class="store-variant-size">' + escapeHtml(String(v.sizeGB)) + ' GB</span><button class="btn-download" data-download-model="' + escapeHtml(String(m.id)) + '" data-download-file="' + escapeHtml(String(v.filename)) + '">Baixar</button></div>';
        }).join('');
        const hiddenVars = m.variants.slice(4).map(v => {
            const name = escapeHtml(String(v.filename).split('/').pop());
            return '<div class="store-variant store-variant-extra" style="display:none"><span class="store-variant-name">' + name + '</span><span class="store-variant-size">' + escapeHtml(String(v.sizeGB)) + ' GB</span><button class="btn-download" data-download-model="' + escapeHtml(String(m.id)) + '" data-download-file="' + escapeHtml(String(v.filename)) + '">Baixar</button></div>';
        }).join('');
        const moreCount = m.variants.length - 4;
        const moreBtn = moreCount > 0 ? '<button type="button" class="store-more-btn" style="color:#fff;background:rgba(124,58,237,0.25);border-top:1px solid #555" data-toggle-variants data-count="' + moreCount + '">+' + moreCount + ' mais variantes</button>' : '';
        return '<div class="store-card"><div class="store-card-header"><div><div class="store-card-name">' + escapeHtml(String(m.name || 'Modelo')) + '</div><div class="store-card-author">by ' + escapeHtml(String(m.author || 'desconhecido')) + '</div></div><div class="store-card-stats"><span>DL: ' + escapeHtml(formatNumber(m.downloads)) + '</span><span>Like: ' + escapeHtml(formatNumber(m.likes)) + '</span></div></div><div class="store-card-tags">' + tags + quants + sizeTag + '</div><div class="store-variants">' + initialVars + hiddenVars + moreBtn + '</div></div>';
    }).join('');
}

function toggleVariants(btn) {
    const card = btn.closest('.store-card');
    const extras = card.querySelectorAll('.store-variant-extra');
    const isHidden = extras[0] && extras[0].style.display === 'none';
    extras.forEach(el => el.style.display = isHidden ? 'flex' : 'none');
    btn.textContent = isHidden ? '- Menos variantes' : '+' + btn.dataset.count + ' mais variantes';
}

async function downloadModel(modelId, filename) {
    try {
        const res = await fetch('/api/models/store/download', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ modelId, filename }) });
        const data = await res.json();
        if (data.success) updateDownloadsPanel();
        else alert('Erro: ' + (data.error || ''));
    } catch(e) { alert('Erro: ' + e.message); }
}

function startDownloadPolling() {
    if (downloadPolling) clearInterval(downloadPolling);
    downloadPolling = setInterval(updateDownloadsPanel, 2000);
}

async function updateDownloadsPanel() {
    try {
        const res = await fetch('/api/models/store/download/progress');
        const data = await res.json();
        const panel = document.getElementById('store-downloads');
        const list = document.getElementById('downloads-list');
        if (!data.success || !data.data.length) { if (panel) panel.style.display = 'none'; return; }
        if (panel) panel.style.display = 'block';
        if (data.data.some(d => d.status === 'complete')) await fetchModels();
        if (list) list.innerHTML = data.data.map(d => {
            const percent = d.status === 'complete' ? 100 : Math.max(0, Math.min(100, Number(d.percent) || 0));
            const status = d.status === 'complete' ? 'Concluído — selecione o modelo na barra lateral' : (d.status === 'error' ? 'Falhou: ' + d.error : percent + '%');
            return '<div class="download-item"><span class="download-name">' + escapeHtml(String(d.filename).split('/').pop()) + '</span><div class="download-progress-bar"><div class="download-progress-fill" style="width:' + percent + '%"></div></div><span class="download-percent">' + escapeHtml(status) + '</span></div>';
        }).join('');
    } catch(e) {}
}

function formatNumber(n) {
    n = Number(n) || 0;
    if (n >= 1000000) return (n/1000000).toFixed(1) + 'M';
    if (n >= 1000) return (n/1000).toFixed(1) + 'K';
    return n;
}

if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initModelStore);
else initModelStore();

// A interface usa alguns handlers criados dinamicamente em HTML. Como app.js é
// carregado como módulo na versão de produção, estes nomes precisam ser
// expostos explicitamente para que os controles continuem funcionando.
Object.assign(window, { toggleSkill, editSkill, deleteSkill });
