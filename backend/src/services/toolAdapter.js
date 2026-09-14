function parseArguments(value) {
    if (!value) return {};
    if (typeof value === 'object') return value;
    try { return JSON.parse(value); } catch { return {}; }
}

function nativeActions(message) {
    const calls = [
        ...(Array.isArray(message && message.tool_calls) ? message.tool_calls : []),
        ...(message && message.function_call ? [{ function: message.function_call }] : [])
    ];
    return calls.map(call => ({
        kind: call && call.function && call.function.name,
        ...parseArguments(call && call.function && call.function.arguments)
    })).filter(action => action.kind);
}

function textActions(content) {
    const text = String(content || '');
    const xml = [...text.matchAll(/<tool_call>\s*<function=([a-z_]+)>\s*([\s\S]*?)<\/function>\s*<\/tool_call>/gi)]
        .map(match => {
            const action = { kind: match[1] };
            for (const parameter of match[2].matchAll(/<parameter=([a-z_]+)>\s*([\s\S]*?)\s*<\/parameter>/gi)) {
                action[parameter[1]] = parseArguments(parameter[2].trim());
                if (Object.keys(action[parameter[1]]).length === 0 && parameter[2].trim()) action[parameter[1]] = parameter[2].trim();
            }
            return action;
        });
    if (xml.length) return xml;
    // O template nativo de diversos modelos Qwen/llama.cpp representa a
    // chamada como <tool_call>{"name":"...","arguments":{...}}</tool_call>.
    // Não é o XML antigo com <function=...>, mas é uma chamada estruturada
    // válida e não deve vazar para a conversa como texto comum.
    const qwen = [...text.matchAll(/<tool_call>\s*(\{[\s\S]*?\})\s*<\/tool_call>/gi)]
        .map(match => parseArguments(match[1]))
        .map(call => ({ kind: call && call.name, ...parseArguments(call && call.arguments) }))
        .filter(action => action.kind);
    if (qwen.length) return qwen;
    // Alguns modelos Qwen geram <lemma> em vez de <tool_call>
    const lemma = [...text.matchAll(/<lemma>\s*(\{[\s\S]*?\})\s*<\/lemma>/gi)]
        .map(match => parseArguments(match[1]))
        .map(call => ({ kind: call && call.name, ...parseArguments(call && call.arguments) }))
        .filter(action => action.kind);
    if (lemma.length) return lemma;
    // Formato XML com <function_calls><function_call><name>...<arguments>...
    const funcXml = [...text.matchAll(/<function_call>\s*<name>([^<]+)<\/name>\s*<arguments>([\s\S]*?)<\/arguments>\s*<\/function_call>/gi)]
        .map(match => {
            const kind = match[1].trim();
            let args = {};
            const argText = match[2].trim();
            try { args = JSON.parse(argText); } catch {
                const pathMatch = argText.match(/<path>([^<]+)<\/path>/);
                const contentMatch = argText.match(/<content>([\s\S]*?)<\/content>/);
                const oldTextMatch = argText.match(/<oldText>([\s\S]*?)<\/oldText>/);
                const newTextMatch = argText.match(/<newText>([\s\S]*?)<\/newText>/);
                const queryMatch = argText.match(/<query>([^<]+)<\/query>/);
                const commandMatch = argText.match(/<command>([^<]+)<\/command>/);
                if (pathMatch) args.path = pathMatch[1].trim();
                if (contentMatch) args.content = contentMatch[1].trim();
                if (oldTextMatch) args.oldText = oldTextMatch[1].trim();
                if (newTextMatch) args.newText = newTextMatch[1].trim();
                if (queryMatch) args.query = queryMatch[1].trim();
                if (commandMatch) args.command = commandMatch[1].trim();
            }
            return { kind, ...args };
        })
        .filter(action => action.kind);
    if (funcXml.length) return funcXml;
    // Suporte para chamadas Python: create_file(path="...", content="...")
    const pythonCall = [...text.matchAll(/\b(create_file|write_file|read_file|replace_text|run_command|list_files|search_project|inspect_project|audit_project)\s*\(([\s\S]*?)\)\s*$/gm)]
        .map(match => {
            const kind = match[1];
            const args = match[2];
            const action = { kind };
            const pathMatch = args.match(/path\s*=\s*["']([^"']+)["']/);
            const contentMatch = args.match(/content\s*=\s*["']([\s\S]*?)["']/);
            const commandMatch = args.match(/command\s*=\s*\[([^\]]+)\]/);
            if (pathMatch) action.path = pathMatch[1];
            if (contentMatch) action.content = contentMatch[1];
            if (commandMatch) action.command = commandMatch[1].split(',').map(s => s.trim().replace(/["']/g, ''));
            return action;
        })
        .filter(action => action.path || action.command);
    if (pythonCall.length) return pythonCall;
    const json = text.match(/```(?:json)?\s*([\s\S]*?)```/i);
    if (json) {
        const payload = parseArguments(json[1].trim());
        if (Array.isArray(payload.actions)) return payload.actions;
        if (payload.kind) return [payload];
        // Alguns GGUFs com template Qwen recebem ferramentas nativas, mas
        // retornam a chamada em um bloco JSON simples em vez de preencher
        // `tool_calls`. É uma ação válida, não uma resposta de conversa.
        if (payload.name && typeof payload.name === 'string') {
            return [{ kind: payload.name, ...parseArguments(payload.arguments) }];
        }
    }
    // Salvamento determinístico: o qwen-kimi 7B frequentemente despeja o
    // código correto em blocos markdown com cabeçalho "# caminho/arquivo.ext"
    // em vez do bloco JSON rígido. Em vez de descartar o trabalho bom, extrai
    // cada fence com cabeçalho de arquivo e o transforma em create_file.
    // Arquivos já existentes serão rejeitados pelo create_file (guardas
    // read-before-write continuam valendo) — nada é sobrescrito às cegas.
    const fenceActions = [];
    for (const fence of text.matchAll(/```(?:[^\n`]*)\n([\s\S]*?)(?:```|$)/g)) {
        const body = fence[1];
        const header = body.match(/^\s*(?:#|--|;|\/\/)\s*([A-Za-z0-9_.\-/]+\.[A-Za-z0-9]+)\s*$/m);
        if (!header) continue;
        const filePath = header[1].trim();
        let fileContent = body.slice(header.index + header[0].length);
        if (/^\s*\r?\n/.test(fileContent)) fileContent = fileContent.replace(/^\s*\r?\n/, '');
        const trimmed = fileContent.trim();
        if (trimmed && !fenceActions.some(a => a.path === filePath)) {
            fenceActions.push({ kind: 'create_file', path: filePath, content: trimmed + '\n' });
        }
    }
    if (fenceActions.length) return fenceActions;
    return [];
}

async function getCapabilities(baseUrl) {
    try {
        const response = await fetch(baseUrl + '/props', { signal: AbortSignal.timeout(2000) });
        if (!response.ok) return { nativeTools: false, supportsSystem: true, template: null };
        const props = await response.json();
        const caps = props.chat_template_caps || {};
        return { nativeTools: !!caps.supports_tools, supportsSystem: caps.supports_system_role !== false, template: props.chat_template || null };
    } catch { return { nativeTools: false, supportsSystem: true, template: null }; }
}

function modelProfile(modelPath, capabilities) {
    return {
        id: String(modelPath || 'default').split(/[\\/]/).pop(),
        nativeTools: !!capabilities.nativeTools,
        supportsSystem: capabilities.supportsSystem !== false,
        mode: capabilities.nativeTools ? 'ferramentas-nativas' : 'ferramentas-estruturadas'
    };
}

module.exports = { nativeActions, textActions, getCapabilities, modelProfile };
