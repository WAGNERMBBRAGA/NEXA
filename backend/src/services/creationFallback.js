const fs = require('fs');
const path = require('path');

function creationBriefAction(projectRoot, objective, plan = {}) {
    const target = path.join(projectRoot, 'NEXA_PROJECT_BRIEF.md');
    const completed = plan.completed || [];
    // Mantém a conversa avançando por etapas verificáveis mesmo quando um
    // modelo local não devolve uma chamada de ferramenta. Não gera código
    // fictício: primeiro preserva o pedido, depois inspeciona e valida.
    if (fs.existsSync(target)) {
        if (!completed.includes('Estrutura do projeto inspecionada')) return [{ kind: 'inspect_project' }];
        if (!completed.includes('Build e testes verificados')) return [{ kind: 'audit_project' }];
        return [];
    }
    const requirements = Array.isArray(plan.requirements) && plan.requirements.length
        ? plan.requirements
        : [objective];
    const content = `# Briefing inicial do projeto\n\n## Objetivo informado\n\n${String(objective || '').trim()}\n\n## Requisitos registrados\n\n${requirements.map((item, index) => `${index + 1}. ${String(item)}`).join('\n')}\n\n## Fluxo de trabalho do NEXA\n\n1. Inspecionar a pasta vinculada.\n2. Criar a base do projeto usando as ações verificáveis do agente.\n3. Validar os arquivos criados e continuar pelas próximas etapas desta conversa.\n\n> Este arquivo foi criado automaticamente porque o modelo ativo não emitiu uma primeira ação de criação válida. Ele preserva o objetivo sem inventar código ou funcionalidades.\n`;
    return [{ kind: 'create_file', path: 'NEXA_PROJECT_BRIEF.md', content }];
}

module.exports = { creationBriefAction };
