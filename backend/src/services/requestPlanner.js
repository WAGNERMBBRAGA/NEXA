function planRequest(message, context = {}) {
    const text = String(message || '');
    const bareContinuation = /^\s*(?:prossiga|continue|avan[çc]e|siga|pr[oó]ximo)\s*[.!]*\s*$/i.test(text);
    const previousProjectWork = !!context.hasProjectWork
        || (context.lastAssistant && Array.isArray(context.lastAssistant.actionResults)
            && context.lastAssistant.actionResults.some(result => ['inspect_project', 'audit_project', 'read_file', 'search_project', 'write_file', 'create_file', 'replace_text', 'run_command'].includes(result.kind)))
        || !!(context.taskState && Array.isArray(context.taskState.completed) && context.taskState.completed.length);
    if (bareContinuation && context.workPlan && context.workPlan.status === 'in_progress') {
        return { actions: [], mode: 'continue_creation' };
    }
    if (bareContinuation && previousProjectWork) {
        return { actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }], mode: 'continue_corrections' };
    }
    const continuesCorrections = /\b(?:prossiga|continue|avan[çc]e|siga)\b[\s\S]*\b(?:corre[çc][õo]es|corrigir|ajustes|consertos)\b|\b(?:corrija|corrigir)\b[\s\S]*\b(?:inconsist[eê]ncias|erros|problemas)\b/i.test(text);
    if (continuesCorrections) {
        return { actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }], mode: 'continue_corrections' };
    }
    if (/\b(?:arquitetura|funcionar\s+perfeitamente|precisa\s+ser\s+feito|o\s+que\s+falta)\b/i.test(text)) {
        return { actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }], mode: 'project_analysis' };
    }
    // Criação só é planejada sem IA quando o pedido traz todos os dados
    // necessários. Isso evita adivinhar arquitetura, nomes ou conteúdo.
    const project = /\b(?:crie|criar)\s+(?:um\s+)?projeto(?:\s+m[ií]nimo)?\s+(?:chamado|com\s+o\s+nome)\s+([\w.-]+)/i.exec(text);
    const file = /\barquivo\s+([\w.-]+(?:[\\/][\w.-]+)*)\s+contendo\s+exatamente\s*:\s*([\s\S]+)$/i.exec(text);
    if (project && file) {
        const projectPath = project[1];
        const filePath = file[1].replace(/\\/g, '/');
        const content = file[2].trim();
        if (!projectPath.includes('..') && !filePath.startsWith('../') && content) {
            return { actions: [{ kind: 'create_project', path: projectPath, files: [{ path: filePath, content }] }] };
        }
    }
    const replace = /\b(?:substitua|altere)\s+(?:todo\s+)?(?:o\s+)?conte[uú]do\s+(?:do\s+)?arquivo\s+([\w.-]+(?:[\\/][\w.-]+)*)\s+(?:por|para)\s*:\s*([\s\S]+)$/i.exec(text);
    if (replace) {
        const filePath = replace[1].replace(/\\/g, '/');
        const content = replace[2].trim();
        if (!filePath.startsWith('../') && content) {
            return { actions: [
                { kind: 'read_file', path: filePath },
                { kind: 'write_file', path: filePath, content }
            ] };
        }
    }
    if (/\b(?:verific|audit|analis|erro|bug|inconsist|estrutura|revis)/i.test(text) && /\b(?:projeto|estrutura|erros|bugs)/i.test(text)) {
        return { actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }], mode: 'project_analysis' };
    }
    const asksToRead = /\b(?:leia|ler|abra|analise|analisar|revise|revisar)\b/i.test(text);
    const paths = [...text.matchAll(/(?:^|[\s`"'])([\w.-]+(?:[\\/][\w.@-]+)+(?:\.[\w-]+)?)/g)]
        .map(match => match[1].replace(/\\/g, '/'))
        .filter(path => !path.startsWith('../') && !path.startsWith('/'));
    if (asksToRead && paths.length) return { actions: [...new Set(paths)].slice(0, 3).map(path => ({ kind: 'read_file', path })) };
    return { actions: [] };
}

module.exports = { planRequest };
