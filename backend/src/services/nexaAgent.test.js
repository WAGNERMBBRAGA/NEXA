const assert = require('node:assert/strict');
const test = require('node:test');
const nexaAgent = require('./nexaAgent');
const { classifyIntent, decide, executionResponse, isControlMessage, projectRequirements } = nexaAgent;

test('recognizes construction continuations as project execution', () => {
    assert.equal(classifyIntent('prossiga com a construção'), 'create');
    const decision = decide({ message: 'prossiga com a construção', workPlan: { status: 'in_progress', completed: [] } });
    assert.equal(decision.modelMayAct, false);
    assert.equal(decision.actions[0].kind, 'inspect_project');
    assert.ok(decision.actions.some(action => action.kind === 'create_file'));
});

test('hands a validated creation to the tool-capable model for the next module', () => {
    const decision = nexaAgent.decide({
        message: 'prossiga com a construção',
        workPlan: {
            status: 'in_progress',
            blueprintCreated: true,
            blueprintValidated: true,
            completed: ['Estrutura do projeto inspecionada', 'Índice contextual atualizado', 'Build e testes verificados', 'Arquivos criados', 'Validações executadas']
        }
    });
    assert.equal(decision.intent, 'create');
    assert.equal(decision.actions.length, 0);
    assert.equal(decision.modelMayAct, true);
    assert.equal(nexaAgent.ownsExecution(decision), false);
});

test('keeps a language recommendation as normal conversation', () => {
    assert.equal(classifyIntent('Qual linguagem você sugere para criar um sistema?'), 'chat');
});

test('does not save creation controls and status questions as requirements', () => {
    for (const message of ['COMEÇAR A CRIAR O PROJETO', 'CONTINUE CRIANDO O SISTEMA', 'PROSSIGA COM A CRIAÇÃO', 'O QUE VOCÊ ESTÁ CRIANDO?']) {
        assert.equal(isControlMessage(message), true, message);
        assert.deepEqual(projectRequirements(message, ['Sistema para academias']), ['Sistema para academias']);
    }
});

test('answers project status from the persisted plan without calling a model', () => {
    const decision = decide({ message: 'O QUE VOCÊ ESTÁ CRIANDO?', workPlan: { status: 'in_progress', completed: ['Arquivos criados'], pending: ['Validar os arquivos criados'] } });
    assert.equal(decision.intent, 'status');
    assert.equal(decision.modelMayAct, false);
    assert.match(executionResponse(decision, [], { completed: ['Arquivos criados'], pending: ['Validar os arquivos criados'] }), /Arquivos criados/);
});

test('returns grounded execution text without model synthesis', () => {
    const content = executionResponse(
        { intent: 'review' },
        [{ kind: 'inspect_project', ok: true, details: { files: 12 } }, { kind: 'audit_project', ok: true, details: { checks: [{ ok: false }] } }],
        null
    );
    assert.match(content, /12 arquivo/);
    assert.match(content, /1 valida/);
});
