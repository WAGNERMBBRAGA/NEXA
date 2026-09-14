const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { auditProject } = require('./projectAudit');
const { inspectProject } = require('./projectInspector');
const { refreshIndex, searchIndex } = require('./projectIndex');
const { buildSemanticIndex, semanticSearch } = require('./semanticIndex');

const MAX_ACTIONS = 20;
const MAX_FILE_BYTES = 512 * 1024;
const MAX_READ_CHARS = 8000;
const MAX_LISTED_FILES = 250;
const MAX_SEARCH_RESULTS = 80;
const ALLOWED_COMMANDS = new Set(['cargo', 'git', 'node', 'npm', 'npx', 'python', 'python3']);
const READ_ONLY_GIT = new Set(['status', 'diff', 'log', 'show', 'branch', 'rev-parse']);
const transactions = new Map();
// Transações de criação podem demorar vários minutos em modelos locais lentos
// (~11 tok/s): MAX_TURNS × CHAT_TIMEOUT_MS ultrapassa facilmente 300s. Um
// timeout menor que a duração da request faz o limpeza expirar a transação no
// meio da execução e o commit reportar "0 arquivo(s)" com arquivos já gravados.
const TRANSACTION_TIMEOUT_MS = 20 * 60 * 1000;

function transactionKey(root) {
    return fs.realpathSync(root).toLocaleLowerCase();
}

function cleanupStaleTransactions() {
    const now = Date.now();
    for (const [key, tx] of transactions) {
        if (now - tx.startedAt > TRANSACTION_TIMEOUT_MS) {
            for (const [target, original] of tx.files) {
                if (original.existed && original.content) {
                    try {
                        fs.mkdirSync(path.dirname(target), { recursive: true });
                        fs.writeFileSync(target, original.content);
                    } catch {}
                } else if (!original.existed) {
                    try { fs.rmSync(target, { force: true }); } catch {}
                }
            }
            transactions.delete(key);
        }
    }
}

setInterval(cleanupStaleTransactions, 60000).unref();

function beginTransaction(root) {
    const key = transactionKey(root);
    cleanupStaleTransactions();
    transactions.set(key, { root: fs.realpathSync(root), files: new Map(), startedAt: Date.now() });
    return { active: true };
}

function captureOriginal(root, target) {
    const transaction = transactions.get(transactionKey(root));
    if (!transaction || transaction.files.has(target)) return;
    transaction.files.set(target, fs.existsSync(target)
        ? { existed: true, content: fs.readFileSync(target) }
        : { existed: false, content: null });
}

function commitTransaction(root) {
    const key = transactionKey(root);
    const transaction = transactions.get(key);
    transactions.delete(key);
    return { committed: !!transaction, files: transaction ? transaction.files.size : 0 };
}

function rollbackTransaction(root) {
    const key = transactionKey(root);
    const transaction = transactions.get(key);
    if (!transaction) return { rolledBack: false, files: [] };
    const restored = [];
    for (const [target, original] of [...transaction.files.entries()].reverse()) {
        if (original.existed) {
            fs.mkdirSync(path.dirname(target), { recursive: true });
            fs.writeFileSync(target, original.content);
        } else if (fs.existsSync(target)) {
            fs.rmSync(target, { force: true });
        }
        restored.push(path.relative(transaction.root, target).replace(/\\/g, '/'));
    }
    transactions.delete(key);
    return { rolledBack: true, files: restored };
}

// Modelos locais às vezes usam aspas tipográficas, comentários explicativos ou
// vírgula final, e — especialmente em GGUFs — emitem quebras de linha reais
// dentro de strings JSON em vez de "\n" escapado. Em JSON isso é inválido e faz
// o JSON.parse derrubar todo o bloco (ou o salvage descartar a última ação).
// Normalizamos só dentro do bloco de ações, preservando o conteúdo de arquivos.
function parseActionJson(value) {
    let text = String(value || '').replace(/[“”]/g, '"').replace(/[‘’]/g, "'");
    let output = ''; let quoted = false; let escaped = false;
    for (let i = 0; i < text.length; i++) {
        const char = text[i];
        if (quoted) {
            if (escaped) { output += char; escaped = false; continue; }
            if (char === '\\') {
                // Modelos locais frequentemente emitem código com backslashes
                // puros (ex.: PHP `\App\Http\Controllers`) dentro do JSON. Em
                // JSON, `\` precisa ser `\\`; um `\A` cru quebra o JSON.parse.
                // Só mantemos `\` como escape se a sequência for válida em JSON.
                const next = text[i + 1];
                if (next === '"' || next === '\\' || next === '/' || next === 'b' || next === 'f' || next === 'n' || next === 'r' || next === 't' || next === 'u') {
                    output += char; escaped = true; continue;
                }
                output += '\\\\'; continue;
            }
            if (char === '"') { output += char; quoted = false; continue; }
            if (char === '\r') { output += '\\r'; continue; }
            if (char === '\n') { output += '\\n'; continue; }
            if (char === '\t') { output += '\\t'; continue; }
            if (char.charCodeAt(0) < 0x20) { output += '\\u' + char.charCodeAt(0).toString(16).padStart(4, '0'); continue; }
            output += char;
            continue;
        }
        if (char === '"') { quoted = true; output += char; continue; }
        if (char === '/' && text[i + 1] === '/') { while (i < text.length && text[i] !== '\n') i++; output += '\n'; continue; }
        if (char === '/' && text[i + 1] === '*') { i += 2; while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) i++; i++; continue; }
        output += char;
    }
    return JSON.parse(output.replace(/,\s*([}\]])/g, '$1'));
}

function parseActions(text) {
    // Alguns templates GGUF representam os delimitadores do protocolo com
    // "\\n" literal. Corrigimos apenas as bordas do bloco, preservando
    // sequências escapadas que pertençam ao conteúdo de arquivos JSON.
    const normalizedText = String(text || '')
        .replace(/:::NEXA_ACTIONS\\n/gi, ':::NEXA_ACTIONS\n')
        .replace(/\\n:::/g, '\n:::');
    const toolMatches = [...normalizedText.matchAll(/<tool_call>\s*<function=([a-z_]+)>\s*([\s\S]*?)<\/function>\s*<\/tool_call>/gi)];
    if (toolMatches.length) {
        const actions = toolMatches.map(match => {
            const action = { kind: match[1] };
            for (const parameter of match[2].matchAll(/<parameter=([a-z_]+)>\s*([\s\S]*?)\s*<\/parameter>/gi)) {
                const value = parameter[2].trim();
                try { action[parameter[1]] = JSON.parse(value); } catch { action[parameter[1]] = value; }
            }
            return action;
        });
        return {
            actions: actions.slice(0, MAX_ACTIONS),
            displayText: normalizedText.replace(/<tool_call>[\s\S]*?<\/tool_call>/gi, '').trim(),
            error: actions.length > MAX_ACTIONS ? `Limite de ${MAX_ACTIONS} ações por resposta excedido.` : null
        };
    }
    const match = /:::NEXA_ACTIONS\s*\n([\s\S]*?)\n:::/i.exec(normalizedText);
    // Alguns modelos envolvem o JSON em fences markdown (```json...```) dentro
    // dos delimitadores. O parse falha com fences; removemos antes de tentar.
    const blockContent = match ? match[1].replace(/^```\w*\n?/i, '').replace(/\n?```\s*$/i, '') : null;
    if (!match) {
        const marker = normalizedText.search(/:{1,3}NEXA_ACTIONS/i);
        if (marker < 0) return { actions: [], displayText: text || '', error: null };
        const jsonStart = normalizedText.indexOf('{', marker);
        if (jsonStart < 0) return { actions: [], displayText: normalizedText.slice(0, marker).trim(), error: 'Bloco de ações sem JSON.' };
        let depth = 0; let quoted = false; let escaped = false; let jsonEnd = -1;
        for (let index = jsonStart; index < normalizedText.length; index++) {
            const char = normalizedText[index];
            if (quoted) {
                if (escaped) escaped = false;
                else if (char === '\\') escaped = true;
                else if (char === '"') quoted = false;
                continue;
            }
            if (char === '"') quoted = true;
            else if (char === '{') depth++;
            else if (char === '}' && --depth === 0) { jsonEnd = index + 1; break; }
        }
        if (jsonEnd < 0) {
            const salvaged = salvageTruncatedBlock(normalizedText.slice(jsonStart), normalizedText.slice(0, marker).trim());
            if (salvaged) return salvaged;
            return { actions: [], displayText: normalizedText.slice(0, marker).trim(), error: 'JSON de ações incompleto.' };
        }
        try {
            const payload = parseActionJson(normalizedText.slice(jsonStart, jsonEnd));
            const candidates = Array.isArray(payload.actions) ? payload.actions
                : Array.isArray(payload.commands) && payload.commands.every(item => item && typeof item.kind === 'string') ? payload.commands
                : null;
            // Alguns modelos externos (ex.: roteia.ai) usam "type" em vez de
            // "kind" no JSON do bloco. Normalizamos para o contrato interno.
            const actions = Array.isArray(candidates) ? candidates.map(action => {
                if (!action || typeof action !== 'object') return action;
                if (typeof action.type === 'string' && typeof action.kind !== 'string') {
                    const normalized = { ...action, kind: action.type };
                    delete normalized.type;
                    return normalized;
                }
                return action;
            }).filter(action => action && typeof action.kind === 'string') : [];
            if (!actions.length) return { actions: [], displayText: normalizedText.slice(0, marker).trim(), error: 'O bloco não contém ações reconhecíveis.' };
            return { actions: actions.slice(0, MAX_ACTIONS), displayText: normalizedText.slice(0, marker).trim(), error: actions.length > MAX_ACTIONS ? `Limite de ${MAX_ACTIONS} ações por resposta excedido.` : null };
        } catch (error) {
            return salvageTruncatedBlock(normalizedText.slice(jsonStart), normalizedText.slice(0, marker).trim()) || { actions: [], displayText: normalizedText.slice(0, marker).trim(), error: `JSON de ações inválido: ${error.message}` };
        }
    }

    const displayText = (normalizedText.slice(0, match.index) + normalizedText.slice(match.index + match[0].length)).trim();
    try {
        const payload = parseActionJson(blockContent);
        const candidates = Array.isArray(payload.actions) ? payload.actions
            : Array.isArray(payload.commands) && payload.commands.every(item => item && typeof item.kind === 'string') ? payload.commands
            : null;
        if (!candidates) {
            return { actions: [], displayText, error: 'O bloco de ações não possui uma lista actions.' };
        }
        if (candidates.length > MAX_ACTIONS) {
            return { actions: [], displayText, error: `Limite de ${MAX_ACTIONS} ações por resposta excedido.` };
        }
        // Aceita "type" como alias de "kind" (formato usado por APIs externas).
        const actions = candidates.map(action => {
            if (!action || typeof action !== 'object') return action;
            if (typeof action.type === 'string' && typeof action.kind !== 'string') {
                const normalized = { ...action, kind: action.type };
                delete normalized.type;
                return normalized;
            }
            return action;
        }).filter(action => action && typeof action.kind === 'string');
        return { actions, displayText, error: null };
    } catch (error) {
        // Modelos locais fecham o objeto raiz {"actions":[...]} corretamente,
        // mas emitem linhas extras antes do `:::` (ex.: um `]}` "fantasma").
        // Em vez de descartar tudo, extrai o primeiro objeto JSON balanceado
        // (respeitando strings/pares escapados) e tenta validá-lo.
        let recoveredPayload;
        let jsonCursor = blockContent.indexOf('{');
        while (jsonCursor >= 0) {
            let depth = 0; let quoted = false; let escaped = false; let objectEnd = -1;
            for (let index = jsonCursor; index < blockContent.length; index++) {
                const char = blockContent[index];
                if (quoted) {
                    if (escaped) escaped = false;
                    else if (char === '\\') escaped = true;
                    else if (char === '"') quoted = false;
                    continue;
                }
                if (char === '"') quoted = true;
                else if (char === '{') depth++;
                else if (char === '}') {
                    depth--;
                    if (depth === 0) { objectEnd = index; break; }
                }
            }
            if (objectEnd < 0) break;
            try {
                const payload = parseActionJson(blockContent.slice(jsonCursor, objectEnd + 1));
                const candidates = Array.isArray(payload.actions) ? payload.actions
                    : Array.isArray(payload.commands) && payload.commands.every(item => item && typeof item.kind === 'string') ? payload.commands
                    : null;
                if (Array.isArray(candidates) && candidates.length) {
                    const recoveredActions = candidates.map(action => {
                        if (!action || typeof action !== 'object') return action;
                        if (typeof action.type === 'string' && typeof action.kind !== 'string') {
                            const normalized = { ...action, kind: action.type };
                            delete normalized.type;
                            return normalized;
                        }
                        return action;
                    }).filter(action => action && typeof action.kind === 'string');
                    if (recoveredActions.length) {
                        recoveredPayload = { actions: recoveredActions.slice(0, MAX_ACTIONS), displayText, error: null };
                        break;
                    }
                }
            } catch { /* objeto incompleto/inválido: tenta o próximo */ }
            jsonCursor = blockContent.indexOf('{', jsonCursor + 1);
        }
        if (recoveredPayload) return recoveredPayload;
        return salvageTruncatedBlock(blockContent, displayText) || { actions: [], displayText, error: `JSON de ações inválido: ${error.message}` };
    }
}

// Um modelo local pode truncar o JSON quando o bloco fica perto do limite de
// tokens. Em vez de descartar tudo, recupera as ações que ficaram completas:
// como o objeto externo {"actions":[...]} fica aberto na truncagem, cada objeto
// "{...}" individual e balanceado é extraído e validado; a última ação cortada
// no meio é descartada e o loop de agentes continua no turno seguinte.
function salvageTruncatedBlock(blockText, displayText) {
    const recovered = [];
    let nextStart = 0;
    while (nextStart < blockText.length) {
        const objectStart = blockText.indexOf('{', nextStart);
        if (objectStart < 0) break;
        let depth = 0;
        let quoted = false;
        let escaped = false;
        let objectEnd = -1;
        for (let index = objectStart; index < blockText.length; index++) {
            const char = blockText[index];
            if (quoted) {
                if (escaped) escaped = false;
                else if (char === '\\') escaped = true;
                else if (char === '"') quoted = false;
                continue;
            }
            if (char === '"') quoted = true;
            else if (char === '{') depth++;
            else if (char === '}') {
                depth--;
                if (depth === 0) { objectEnd = index; break; }
            }
        }
        nextStart = objectStart + 1;
        if (objectEnd < 0) continue;
        try {
            const action = parseActionJson(blockText.slice(objectStart, objectEnd + 1));
            if (action && typeof action.kind === 'string' && !Array.isArray(action)) {
                // Mutações de arquivo sem `path` são objetos truncados/inválidos:
                // recuperá-las gera N ações que falham e desviam o pedido para o
                // fallback. Só salva ações completas e estruturalmente viáveis.
                const needsPath = /^(?:create_file|write_file|replace_text|read_file)$/.test(action.kind);
                if (!needsPath || typeof action.path === 'string' && action.path.trim()) recovered.push(action);
            }
        } catch { /* objeto truncado ou inválido: ignora */ }
    }
    if (!recovered.length) return null;
    return {
        actions: recovered.slice(0, MAX_ACTIONS),
        displayText,
        error: null
    };
}

function projectPath(root, relativePath) {
    if (typeof relativePath !== 'string' || !relativePath.trim()) {
        throw new Error('O caminho relativo do arquivo é obrigatório.');
    }
    if (path.isAbsolute(relativePath)) {
        throw new Error('Caminhos absolutos não são permitidos para ações do agente.');
    }
    const resolvedRoot = fs.realpathSync(root);
    const resolved = path.resolve(resolvedRoot, relativePath);
    const relative = path.relative(resolvedRoot, resolved);
    if (relative === '' || relative.startsWith('..') || path.isAbsolute(relative)) {
        throw new Error('Ação fora da raiz do projeto foi bloqueada.');
    }
    let existingAncestor = resolved;
    while (!fs.existsSync(existingAncestor)) {
        const parent = path.dirname(existingAncestor);
        if (parent === existingAncestor) break;
        existingAncestor = parent;
    }
    const physicalAncestor = fs.realpathSync(existingAncestor);
    const physicalRelative = path.relative(resolvedRoot, physicalAncestor);
    if (physicalRelative.startsWith('..') || path.isAbsolute(physicalRelative)) {
        throw new Error('Ação através de link simbólico fora do projeto foi bloqueada.');
    }
    return resolved;
}

function writeFile(root, action) {
    if (typeof action.content !== 'string') throw new Error('O conteúdo do arquivo deve ser texto.');
    if (Buffer.byteLength(action.content, 'utf8') > MAX_FILE_BYTES) {
        throw new Error(`Arquivo excede o limite de ${MAX_FILE_BYTES} bytes.`);
    }
    const target = projectPath(root, action.path);
    let beforePreview = '';
    if (fs.existsSync(target) && fs.statSync(target).isFile()) {
        try { beforePreview = fs.readFileSync(target, 'utf8').slice(0, 400); } catch {}
    }
    captureOriginal(root, target);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    const temporary = path.join(path.dirname(target), `.${path.basename(target)}.${process.pid}.${Date.now()}.tmp`);
    try {
        fs.writeFileSync(temporary, action.content, 'utf8');
        fs.renameSync(temporary, target);
    } finally {
        if (fs.existsSync(temporary)) fs.rmSync(temporary, { force: true });
    }
    return {
        path: path.relative(root, target),
        bytes: Buffer.byteLength(action.content, 'utf8'),
        beforePreview,
        preview: action.content.slice(0, 400),
        previewTruncated: action.content.length > 400
    };
}

function replaceText(root, action) {
    if (typeof action.oldText !== 'string' || !action.oldText) throw new Error('replace_text exige o trecho original.');
    if (typeof action.newText !== 'string') throw new Error('replace_text exige o novo trecho.');
    const target = projectPath(root, action.path);
    if (!fs.existsSync(target) || !fs.statSync(target).isFile()) throw new Error('Arquivo solicitado não foi encontrado.');
    const content = fs.readFileSync(target, 'utf8');
    const first = content.indexOf(action.oldText);
    if (first < 0) throw new Error('O trecho original não foi encontrado; nenhuma alteração foi aplicada.');
    if (content.indexOf(action.oldText, first + action.oldText.length) >= 0) throw new Error('O trecho original aparece mais de uma vez; informe um trecho mais específico.');
    const updated = content.slice(0, first) + action.newText + content.slice(first + action.oldText.length);
    return { ...writeFile(root, { path: action.path, content: updated }), replaced: true };
}

function readFile(root, action) {
    const target = projectPath(root, action.path);
    if (!fs.existsSync(target) || !fs.statSync(target).isFile()) {
        throw new Error('Arquivo solicitado não foi encontrado.');
    }
    const content = fs.readFileSync(target, 'utf8');
    if (content.includes('\0')) throw new Error('Arquivos binários não podem ser enviados ao modelo.');
    const truncated = content.length > MAX_READ_CHARS;
    return {
        path: path.relative(root, target),
        content: truncated ? content.slice(0, MAX_READ_CHARS) : content,
        truncated
    };
}

// Estrutura é uma ferramenta de descoberta: não lê conteúdo, ignora diretórios
// pesados e nunca permite sair da raiz que está vinculada à conversa.
function listFiles(root, action = {}) {
    const requested = typeof action.path === 'string' && action.path.trim() ? action.path : '.';
    const directory = requested === '.' ? root : projectPath(root, requested);
    if (!fs.existsSync(directory) || !fs.statSync(directory).isDirectory()) {
        throw new Error('Diretório solicitado não foi encontrado.');
    }
    const files = [];
    const ignored = new Set(['node_modules', '.git', 'vendor', 'dist', 'build', 'coverage', '.next', 'target']);
    const walk = current => {
        if (files.length >= MAX_LISTED_FILES) return;
        for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
            if (files.length >= MAX_LISTED_FILES) return;
            if (entry.name === '.' || entry.name === '..') continue;
            const fullPath = path.join(current, entry.name);
            const relative = path.relative(root, fullPath).replace(/\\/g, '/');
            if (entry.isDirectory()) {
                if (!ignored.has(entry.name) && !entry.name.startsWith('.')) walk(fullPath);
            } else if (entry.isFile()) {
                files.push(relative);
            }
        }
    };
    walk(directory);
    return { path: requested === '.' ? '.' : path.relative(root, directory), files, truncated: files.length >= MAX_LISTED_FILES };
}

function searchProject(root, action = {}) {
    const query = typeof action.query === 'string' ? action.query.trim() : '';
    if (!query || query.length > 200) throw new Error('search_project exige uma busca de até 200 caracteres.');
    if (action.path && action.path !== '.') throw new Error('A pesquisa indexada opera na raiz completa do projeto.');
    return { path: '.', ...searchIndex(root, query, MAX_SEARCH_RESULTS) };
}

function createProject(root, action) {
    const directory = projectPath(root, action.path);
    if (!Array.isArray(action.files) || action.files.length === 0) {
        throw new Error('create_project exige ao menos um arquivo.');
    }
    if (action.files.length > MAX_ACTIONS) throw new Error('Projeto contém arquivos demais para uma ação.');
    fs.mkdirSync(directory, { recursive: true });
    const files = action.files.map(file => writeFile(directory, file));
    return { path: path.relative(root, directory), files };
}

function documentDevelopmentModes(root) {
    const target = projectPath(root, 'DEVELOPMENT.md');
    if (fs.existsSync(target)) {
        return { path: 'DEVELOPMENT.md', created: false, reason: 'O arquivo DEVELOPMENT.md já existe e não foi sobrescrito.' };
    }
    const content = `# Ambientes de desenvolvimento\n\n## Modo local padrão\n\nO ambiente local usa SQLite como banco de dados, cache em arquivo e filas síncronas. Esse modo funciona sem Docker e preserva a configuração ativa em \`backend/.env\`.\n\n## Modo Docker opcional\n\nO arquivo \`docker-compose.yml\` oferece uma stack alternativa com MySQL e Redis. Use esse modo somente quando Docker estiver instalado e ajuste as variáveis de ambiente para os serviços declarados no Compose.\n\nOs dois modos são alternativas; não devem ser combinados na mesma execução.\n`;
    return { ...writeFile(root, { path: 'DEVELOPMENT.md', content }), created: true };
}

function runCommand(root, action) {
    // Modelos de API costumam enviar `command` como string única ("python
    // -m unittest ...") em vez de lista ["python", "-m", "unittest", "..."]. 
    // Normalizamos ambos os formatos para o contrato interno.
    let command = action.command;
    if (typeof command === 'string' && command.trim()) {
        command = command.trim().split(/\s+/);
    }
    if (!Array.isArray(command) || command.length === 0 || !command.every(v => typeof v === 'string')) {
        return Promise.reject(new Error('run_command exige command como lista de argumentos.'));
    }
    const [program, ...args] = command;
    if (!ALLOWED_COMMANDS.has(program.toLowerCase())) {
        return Promise.reject(new Error(`Comando '${program}' não faz parte da política do agente.`));
    }
    if (program.toLowerCase() === 'git' && !READ_ONLY_GIT.has((args[0] || '').toLowerCase())) {
        return Promise.reject(new Error("Ações mutáveis do Git, como 'git init', são bloqueadas para o agente."));
    }
    const cwd = action.cwd ? projectPath(root, action.cwd) : root;
    const timeout = Math.min(Math.max(Number(action.timeoutMs) || 30000, 1000), 120000);

    return new Promise((resolve, reject) => {
        const child = spawn(program, args, { cwd, shell: false, windowsHide: true });
        let stdout = '';
        let stderr = '';
        let timedOut = false;
        const timer = setTimeout(() => {
            timedOut = true;
            child.kill();
        }, timeout);
        child.stdout.on('data', chunk => { stdout += chunk.toString(); });
        child.stderr.on('data', chunk => { stderr += chunk.toString(); });
        child.on('error', reject);
        child.on('close', code => {
            clearTimeout(timer);
            resolve({ command, cwd: path.relative(root, cwd) || '.', exitCode: code, timedOut, stdout: stdout.slice(0, 8000), stderr: stderr.slice(0, 8000) });
        });
    });
}

async function executeActions(root, actions) {
    if (!root || !fs.existsSync(root) || !fs.statSync(root).isDirectory()) {
        return actions.map(action => ({ kind: action.kind || 'unknown', ok: false, error: 'Nenhum projeto válido foi selecionado.' }));
    }
    const results = [];
    for (const action of actions) {
        try {
            let details;
            if (action.kind === 'read_file') details = readFile(root, action);
            else if (action.kind === 'list_files') details = listFiles(root, action);
            else if (action.kind === 'search_project') details = searchProject(root, action);
            else if (action.kind === 'index_project') details = refreshIndex(root);
            else if (action.kind === 'build_semantic_index') details = await buildSemanticIndex(root);
            else if (action.kind === 'semantic_search') details = await semanticSearch(root, String(action.query || ''), Number(action.limit) || 10);
            else if (action.kind === 'write_file') details = writeFile(root, action);
            else if (action.kind === 'create_file') {
                const target = projectPath(root, action.path);
                if (fs.existsSync(target)) throw new Error(`O arquivo '${action.path}' já existe; leia-o e use write_file para alterá-lo.`);
                details = writeFile(root, action);
            }
            else if (action.kind === 'replace_text') details = replaceText(root, action);
            else if (action.kind === 'create_project') details = createProject(root, action);
            else if (action.kind === 'document_development_modes') details = documentDevelopmentModes(root);
            else if (action.kind === 'inspect_project') details = inspectProject(root);
            else if (action.kind === 'audit_project') details = auditProject(root);
            else if (action.kind === 'run_command') details = await runCommand(root, action);
            else throw new Error(`Tipo de ação '${action.kind}' não é suportado.`);
            const ok = action.kind === 'run_command'
                ? (details.exitCode === 0 && !details.timedOut)
                : action.kind === 'audit_project'
                    ? (details.unreadable.length === 0 && details.checks.every(check => check.ok))
                    : ['build_semantic_index', 'semantic_search'].includes(action.kind)
                        ? details.available === true
                    : true;
            const commandError = action.kind === 'audit_project'
                ? 'A revisão encontrou validações pendentes ou indisponíveis.'
                : ['build_semantic_index', 'semantic_search'].includes(action.kind)
                    ? (details.reason || 'O provedor de embeddings não está disponível.')
                : details.timedOut ? 'Comando excedeu o tempo máximo permitido.' : `Comando retornou código ${details.exitCode}.`;
            results.push({ kind: action.kind, ok, details, error: ok ? null : commandError });
        } catch (error) {
            results.push({ kind: action && action.kind ? action.kind : 'unknown', ok: false, error: error.message });
        }
    }
    return results;
}

module.exports = { parseActions, executeActions, projectPath, beginTransaction, commitTransaction, rollbackTransaction };
