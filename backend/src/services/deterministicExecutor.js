function creationActions(plan = {}) {
    const completed = plan.completed || [];
    if (!completed.includes('Estrutura do projeto inspecionada')) return [{ kind: 'inspect_project' }];
    if (!completed.includes('Índice contextual atualizado')) return [{ kind: 'index_project' }];
    if (!completed.includes('Build e testes verificados')) return [{ kind: 'audit_project' }];
    return [];
}

function creationStage(plan = {}) {
    const completed = plan.completed || [];
    if (!completed.includes('Estrutura do projeto inspecionada')) return 'inspect';
    if (!completed.includes('Índice contextual atualizado')) return 'index';
    if (!completed.includes('Build e testes verificados')) return 'validate';
    return 'ready_for_implementation';
}

function nextDeterministicActions({ creating, continuing, plan, objective, projectRoot }) {
    if (!creating && !continuing) return [];
    // Nunca criamos um README genérico para fingir que um sistema foi criado.
    // A criação só passa para escrita depois que a conversa tiver uma
    // arquitetura confirmada e um blueprint concreto.
    const requested = [plan && plan.objective, ...(plan && plan.requirements || [])].join('\n');
    const isRestaurant = /restaurante|comanda|gar[çc]om|delivery|rod[ií]zio|lanchonete|mesa/i.test(requested);
    if ((creating || continuing) && isRestaurant) return restaurantBootstrapActions(projectRoot);
    // Uma base já validada não deve voltar para descoberta genérica: ela
    // avança diretamente para o módulo do domínio solicitado.
    if (plan && plan.blueprintValidated && isRestaurant) {
        return restaurantBootstrapActions(projectRoot);
    }
    const discovery = creationActions(plan);
    // Uma ordem explícita de criação em uma pasta nova percorre a fundação
    // completa em uma única etapa verificável. `create_file` nunca sobrescreve
    // conteúdo existente, portanto a sequência continua segura.
    // Só a primeira criação de uma pasta vazia executa descoberta, base e
    // validação em conjunto. Nas continuações, cada descoberta pendente é
    // concluída isoladamente; assim arquivos já criados nunca são enviados de
    // novo para `create_file`.
    if ((creating || continuing) && discovery.length && !(plan && (plan.completed || []).length)) {
        const blueprint = createBlueprint({ ...plan, objective: plan.objective || objective });
        return [
            { kind: 'inspect_project' },
            { kind: 'index_project' },
            { kind: 'audit_project' },
            ...blueprint.files.map(file => ({ kind: 'create_file', path: file.path, content: file.content })),
            blueprint.validation
        ];
    }
    if (discovery.length) return discovery;
    if (plan && !plan.blueprintCreated) {
        const blueprint = createBlueprint({ ...plan, objective: plan.objective || objective });
        return blueprint.files.map(file => ({ kind: 'create_file', path: file.path, content: file.content }));
    }
    if (plan && plan.blueprintCreated && !plan.blueprintValidated) {
        return [{ kind: 'run_command', command: ['node', '--check', 'src/server.js'], timeoutMs: 30000 }];
    }
    if (isRestaurant) {
        return restaurantBootstrapActions(projectRoot);
    }
    return [];
}

module.exports = { nextDeterministicActions, creationStage };
const { createBlueprint } = require('./projectBlueprint');
const { restaurantBootstrapActions } = require('./restaurantBootstrap');
