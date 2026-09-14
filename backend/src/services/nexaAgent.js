const { nextDeterministicActions } = require('./deterministicExecutor');

function classifyIntent(message) {
    const text = String(message || '').toLowerCase();
    if (/\b(?:crie|construa|constru[çc][ãa]o|monte|inicie|iniciar|comece|come[çc]ar)\b/.test(text)) return 'create';
    if (/\b(?:corrija|corrigir|erro|bug|inconsist)/.test(text)) return 'repair';
    if (/\b(?:analise|analisar|audite|auditar|estrutura|arquitetura)/.test(text)) return 'review';
    if (/\b(?:o\s+que\s+(?:voc[eê]|est[aá])\s+(?:est[aá]\s+)?criando|qual\s+(?:a\s+)?etapa|status\s+(?:do\s+)?projeto)\b/.test(text)) return 'status';
    if (/^\s*(?:prossiga|continue|avan[çc]e|siga)\b/.test(text)) return 'continue';
    return 'chat';
}

function decide({ message, workPlan, projectRoot }) {
    const classifiedIntent = classifyIntent(message);
    const continuationInsideRequirement = !!(classifiedIntent === 'chat' && workPlan && workPlan.status === 'in_progress' && /\bprossiga\b/i.test(String(message || '')));
    const intent = continuationInsideRequirement ? 'continue' : classifiedIntent;
    const inProgress = workPlan && workPlan.status === 'in_progress';
    if (intent === 'create' || (intent === 'continue' && inProgress)) {
        const actions = nextDeterministicActions({ creating: intent === 'create', continuing: intent === 'continue', plan: workPlan, objective: message, projectRoot });
        // A descoberta e a base inicial são determinísticas. Depois disso a
        // próxima evolução depende do pedido real e deve usar as ferramentas
        // nativas do modelo, em vez de repetir "prossiga" sem fazer nada.
        return { intent, actions, modelMayAct: actions.length === 0 };
    }
    if (intent === 'review') return { intent, actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }], modelMayAct: false };
    if (intent === 'repair') return { intent, actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }], modelMayAct: false };
    if (intent === 'status' && inProgress) return { intent, actions: [], modelMayAct: false };
    return { intent, actions: [], modelMayAct: true };
}

function executionResponse(decision, results, workPlan) {
    const failed = (results || []).filter(result => !result.ok);
    const inspection = (results || []).find(result => result.kind === 'inspect_project' && result.ok);
    const audit = (results || []).find(result => result.kind === 'audit_project' && result.ok);
    if (failed.length) return `A etapa foi executada, mas ${failed.length} ação(ões) precisa(m) de atenção. O diagnóstico verificável foi registrado no canvas; nenhum resultado foi inventado.`;
    if (decision.intent === 'review' || decision.intent === 'repair') {
        const count = inspection && inspection.details ? inspection.details.files || 0 : 0;
        const checks = audit && audit.details && Array.isArray(audit.details.checks) ? audit.details.checks.filter(check => !check.ok).length : 0;
        return `A revisão verificável foi concluída: ${count} arquivo(s) foram inspecionados e ${checks} validação(ões) requer(em) atenção. Os achados comprovados estão no canvas.`;
    }
    if (decision.intent === 'status' && workPlan) {
        const completed = (workPlan.completed || []).join(', ') || 'nenhuma etapa verificável ainda';
        const pending = (workPlan.pending || []).join(', ') || 'nenhuma etapa pendente registrada';
        const blueprint = workPlan.blueprint && workPlan.blueprint.summary ? ` Base atual: ${workPlan.blueprint.summary}` : '';
        return `O NEXA está criando o projeto vinculado a esta conversa. Já foi concluído: ${completed}.${blueprint} Em seguida: ${pending}.`;
    }
    const changed = (results || []).filter(result => result.ok && ['create_file', 'write_file', 'replace_text'].includes(result.kind));
    if (changed.length) {
        const paths = changed.map(result => (result.details || {}).path).filter(Boolean).slice(0, 6);
        return `Implementação aplicada: ${changed.length} arquivo(s) criados ou atualizados.${paths.length ? ` Arquivos: ${paths.join(', ')}.` : ''} As validações executadas e a prévia atual ficam no canvas.`;
    }
    if (audit) return 'As validações reais da pasta vinculada foram concluídas. Os achados comprovados e os próximos passos estão no canvas.';
    if (inspection) {
        const details = inspection.details || {};
        return `A pasta vinculada foi inspecionada: ${details.files || 0} arquivo(s) e ${details.directories || 0} pasta(s). Nenhuma alteração foi aplicada nesta etapa.`;
    }
    if (workPlan && workPlan.blueprintValidated) return `A base executável já foi validada. Ela inclui ${((workPlan.blueprint && workPlan.blueprint.modules) || []).join(', ') || 'os módulos iniciais'}. O próximo incremento será guiado pelos requisitos concretos desta conversa.`;
    if (workPlan && workPlan.status === 'in_progress') return 'A descoberta da pasta foi concluída. O NEXA manteve o objetivo e os requisitos desta conversa; a próxima etapa é definir o blueprint antes de criar arquivos reais.';
    return 'A etapa foi registrada no canvas com resultados verificáveis.';
}

function ownsExecution(decision) {
    return !!decision && (decision.intent === 'status'
        || ['review', 'repair'].includes(decision.intent)
        || (['create', 'continue'].includes(decision.intent) && Array.isArray(decision.actions) && decision.actions.length > 0));
}

function isControlMessage(message) {
    const value = String(message || '').trim().toLowerCase();
    return /^(?:prossiga|continue|avan[çc]e|siga|pr[oó]ximo|comece|come[çc]ar|inicie|iniciar)(?:\s+(?:com\s+)?(?:a\s+)?(?:cria[çc][ãa]o|constru[çc][ãa]o|criando(?:\s+(?:o\s+)?(?:projeto|sistema))?|criar(?:\s+(?:o\s+)?(?:projeto|sistema))?|o\s+projeto|o\s+sistema))?[!.]*$/.test(value)
        || /^(?:o\s+que\s+(?:voc[eê]|est[aá])\s+(?:est[aá]\s+)?criando|qual\s+(?:a\s+)?etapa|status\s+(?:do\s+)?projeto)[?!\.]*$/.test(value);
}

function projectRequirements(message, existing = []) {
    const value = String(message || '').trim();
    if (!value || isControlMessage(value)) return existing;
    return [...new Set([...(existing || []), value])].slice(-20);
}

module.exports = { classifyIntent, decide, ownsExecution, projectRequirements, executionResponse, isControlMessage };
