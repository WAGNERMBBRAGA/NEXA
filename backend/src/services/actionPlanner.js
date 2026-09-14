function prepareActions(actions, previousResults = []) {
    const inspected = new Set(previousResults
        .filter(result => result.kind === 'read_file' && result.ok && result.details && result.details.path)
        .map(result => result.details.path.replace(/\\/g, '/')));
    const allowed = [];
    const rejected = [];
    const blockReads = new Set();
    for (const action of actions || []) {
        const target = typeof action.path === 'string' ? action.path.replace(/\\/g, '/') : null;
        if (action.kind === 'read_file' && target) {
            inspected.add(target);
            blockReads.add(target);
        }
    }
    for (const action of actions || []) {
        const target = typeof action.path === 'string' ? action.path.replace(/\\/g, '/') : null;
        if (['write_file', 'replace_text'].includes(action.kind) && target && !inspected.has(target) && !blockReads.has(target)) {
            rejected.push({ kind: action.kind, ok: false, error: `Leia '${action.path}' antes de alterá-lo.` });
            continue;
        }
        // Um arquivo de código vazio não é trabalho útil: é um esqueleto que o
        // modelo local emite quando o pedido é grande demais para o orçamento.
        // Rejeita para que ele regenere com conteúdo de verdade no turno seguinte.
        if (action.kind === 'create_file' && typeof action.content === 'string' && action.content.trim().length === 0) {
            rejected.push({ kind: action.kind, ok: false, error: `Arquivo '${action.path}' criado vazio; gere o conteúdo de verdade.` });
            continue;
        }
        allowed.push(action);
    }
    return { allowed, rejected };
}

module.exports = { prepareActions };
