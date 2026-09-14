const test = require('node:test');
const assert = require('node:assert/strict');
const { runAgentLoop, compactActionResults } = require('./agentLoop');

test('completes the agent cycle: read, edit, validate, then natural response', async () => {
    const replies = [
        { success: true, data: { content: '', actions: [{ kind: 'read_file', path: 'src/app.js' }] } },
        { success: true, data: { content: '', actions: [{ kind: 'write_file', path: 'src/app.js', content: 'export default 2;' }, { kind: 'run_command', command: ['node', '--check', 'src/app.js'] }] } },
        { success: true, data: { content: 'Corrigi o arquivo e validei a sintaxe.' } }
    ];
    const executed = [];
    const result = await runAgentLoop({
        request: { prompt: 'Corrija src/app.js', systemPrompt: 'agent', history: [] },
        chat: async () => replies.shift(),
        executeActions: async actions => {
            executed.push(...actions);
            return actions.map(action => ({ kind: action.kind, ok: true, details: { path: action.path, command: action.command } }));
        }
    });
    assert.deepEqual(executed.map(action => action.kind), ['read_file', 'write_file', 'run_command']);
    assert.equal(result.actionResults.filter(result => result.ok).length, 3);
});

test('completes the agent cycle for a new project', async () => {
    const replies = [
        { success: true, data: { content: '', actions: [{ kind: 'create_project', path: 'novo-app', files: [{ path: 'README.md', content: '# Novo app' }] }] } },
        { success: true, data: { content: '', actions: [{ kind: 'run_command', command: ['node', '--version'] }] } },
        { success: true, data: { content: 'Criei a estrutura inicial e validei o ambiente.' } }
    ];
    const result = await runAgentLoop({
        request: { prompt: 'Crie um novo projeto', systemPrompt: 'agent', history: [] },
        chat: async () => replies.shift(),
        executeActions: async actions => actions.map(action => ({ kind: action.kind, ok: true, details: { path: action.path, command: action.command } }))
    });
    assert.equal(result.actionResults.length, 2);
    assert.equal(result.content, 'Criei a estrutura inicial e validei o ambiente.');
});

test('allows separate read, edit, validation and final-answer turns', async () => {
    const replies = [
        { success: true, data: { content: '', actions: [{ kind: 'read_file', path: 'a.js' }] } },
        { success: true, data: { content: '', actions: [{ kind: 'write_file', path: 'a.js', content: 'ok' }] } },
        { success: true, data: { content: '', actions: [{ kind: 'run_command', command: ['node', '--check', 'a.js'] }] } },
        { success: true, data: { content: 'Correção concluída e validada.' } }
    ];
    const result = await runAgentLoop({
        request: { prompt: 'Corrija a.js', history: [] },
        maxTurns: 4,
        chat: async () => replies.shift(),
        executeActions: async actions => actions.map(action => ({ kind: action.kind, ok: true, details: { path: action.path, command: action.command } }))
    });
    assert.equal(result.actionResults.length, 3);
    assert.equal(result.content, 'Correção concluída e validada.');
});

test('keeps verified findings in conversation history for the next correction', () => {
    const compact = compactActionResults([{ kind: 'inspect_project', ok: true, details: {
        findings: [{ severity: 'warning', code: 'missing_tests', file: '.' }],
        architecture: { ecosystems: ['Python'], entryPoints: ['app.py'] },
        manifests: ['requirements.txt']
    } }]);
    assert.equal(compact[0].details.findings[0].code, 'missing_tests');
    assert.deepEqual(compact[0].details.architecture.entryPoints, ['app.py']);
});
