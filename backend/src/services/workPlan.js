function isContinuation(objective) {
    return /^\s*(?:prossiga|continue|avan[çc]e|siga|pr[oó]ximo)\b/i.test(String(objective || ''));
}

function updateWorkPlan(previous, taskState) {
    const current = taskState || { objective: '', status: 'complete', completed: [], pending: [] };
    const keepObjective = previous && isContinuation(current.objective);
    const completed = [...new Set([...(keepObjective ? previous.completed || [] : []), ...(current.completed || [])])];
    const creationInProgress = previous && previous.status === 'in_progress';
    const basePending = creationInProgress ? [
        'Inspecionar a pasta vinculada',
        'Atualizar o índice contextual',
        'Validar a estrutura existente',
        'Definir o blueprint do sistema',
        'Implementar os módulos e validar cada etapa'
    ] : (current.pending || []);
    const doneLabels = new Set(completed);
    const pending = creationInProgress
        ? basePending.filter(item => !(
            (item.startsWith('Inspecionar') && doneLabels.has('Estrutura do projeto inspecionada')) ||
            (item.startsWith('Atualizar') && doneLabels.has('Índice contextual atualizado')) ||
            (item.startsWith('Validar') && doneLabels.has('Build e testes verificados'))
        ))
        : [...new Set(basePending)];
    const blueprintCreated = !!(previous && previous.blueprintCreated) || (current.completed || []).includes('Arquivos criados');
    const blueprintValidated = !!(previous && previous.blueprintValidated) || (current.completed || []).includes('Validações executadas');
    return {
        objective: keepObjective ? previous.objective : current.objective,
        status: creationInProgress ? 'in_progress' : (pending.length ? 'attention' : 'complete'),
        completed,
        pending,
        requirements: previous && Array.isArray(previous.requirements) ? previous.requirements : [],
        blueprintCreated,
        blueprintValidated,
        blueprint: blueprintCreated ? {
            id: 'node-service',
            summary: 'Base executável inicial com saúde do serviço e cadastro em memória.',
            modules: ['API HTTP', 'Saúde do serviço', 'Cadastro inicial']
        } : null,
        updatedAt: new Date().toISOString()
    };
}

module.exports = { updateWorkPlan };
