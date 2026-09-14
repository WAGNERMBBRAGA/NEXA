const assert = require('node:assert/strict');
const test = require('node:test');
const { creationStage, nextDeterministicActions } = require('./deterministicExecutor');

test('advances a creation only through deterministic verified stages', () => {
    assert.equal(creationStage({}), 'inspect');
    const initial = nextDeterministicActions({ creating: true, plan: {} });
    assert.equal(initial[0].kind, 'inspect_project');
    assert.ok(initial.some(action => action.kind === 'create_file'));
    assert.ok(initial.some(action => action.kind === 'run_command'));
    assert.equal(creationStage({ completed: ['Estrutura do projeto inspecionada'] }), 'index');
    assert.equal(creationStage({ completed: ['Estrutura do projeto inspecionada', 'Índice contextual atualizado'] }), 'validate');
});

test('does not create placeholder files after validation', () => {
    const plan = { completed: ['Estrutura do projeto inspecionada', 'Índice contextual atualizado', 'Build e testes verificados'] };
    const actions = nextDeterministicActions({ creating: true, plan, objective: 'Criar sistema para academias' });
    assert.ok(actions.some(action => action.path === 'src/server.js'));
});

test('validates a generated blueprint before advancing', () => {
    const plan = { blueprintCreated: true, completed: ['Estrutura do projeto inspecionada', 'Índice contextual atualizado', 'Build e testes verificados'] };
    assert.deepEqual(nextDeterministicActions({ continuing: true, plan }), [{ kind: 'run_command', command: ['node', '--check', 'src/server.js'], timeoutMs: 30000 }]);
});
