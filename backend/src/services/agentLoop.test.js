const test = require('node:test');
const assert = require('node:assert/strict');
const { runAgentLoop, compactActionResults, looksLikeInternalLeak } = require('./agentLoop');

test('continues after action results and returns the verified final answer', async () => {
    const replies = [
        { success: true, data: { content: 'Vou validar.\n:::NEXA_ACTIONS\n{"actions":[{"kind":"run_command","command":["node","--version"]}]}\n:::' } },
        { success: true, data: { content: 'Validação concluída com sucesso.' } }
    ];
    const prompts = [];
    const result = await runAgentLoop({
        request: { prompt: 'Valide o projeto.' },
        chat: async request => { prompts.push(request.prompt); return replies.shift(); },
        executeActions: async actions => [{ kind: actions[0].kind, ok: true, details: { exitCode: 0 } }]
    });

    assert.equal(result.content, 'Validação concluída com sucesso.');
    assert.equal(result.actionResults.length, 1);
    assert.equal(result.actionResults[0].ok, true);
    assert.match(prompts[1], /already ran/);
    assert.match(prompts[1], /untrusted/);
});

test('stops immediately when no action is requested', async () => {
    const result = await runAgentLoop({
        request: { prompt: 'Explique Rust.' },
        chat: async () => ({ success: true, data: { content: 'Rust usa ownership.' } }),
        executeActions: async () => { throw new Error('não deveria executar'); }
    });
    assert.equal(result.content, 'Rust usa ownership.');
    assert.deepEqual(result.actionResults, []);
});

test('retries when no action is produced for an authorized request', async () => {
    let calls = 0;
    const result = await runAgentLoop({
        request: { prompt: 'Vamos criar um sistema.', requireAction: true },
        chat: async () => { calls++; return { success: true, data: { content: 'Posso sugerir uma arquitetura.' } }; },
        executeActions: async () => { throw new Error('não deveria executar'); }
    });
    assert.ok(calls > 1, 'should retry when no action is produced');
    assert.match(result.content, /arquitetura/);
    assert.deepEqual(result.actionResults, []);
});

test('retries instead of showing leaked tool instructions in normal chat', async () => {
    const replies = [
        { success: true, data: { content: 'Use backup_project e retorne <result> XML. LangChain tool function.' } },
        { success: true, data: { content: 'Olá! Como posso ajudar?' } }
    ];
    const result = await runAgentLoop({ request: { prompt: 'Oi' }, chat: async () => replies.shift(), executeActions: async () => [] });
    assert.equal(result.content, 'Olá! Como posso ajudar?');
});

test('rejects unrelated feeds and notebook fragments from a normal answer', async () => {
    const replies = [
        { success: true, data: { content: 'RSS feed: https://one.test/ e https://two.test/ <jupyter output>' } },
        { success: true, data: { content: 'Sim, posso ajudar com programação.' } }
    ];
    const result = await runAgentLoop({ request: { prompt: 'Você sabe programar?' }, chat: async () => replies.shift(), executeActions: async () => [] });
    assert.equal(result.content, 'Sim, posso ajudar com programação.');
});

test('rejects role wrappers and internal project-action previews from normal chat', () => {
    assert.equal(looksLikeInternalLeak('Vou usar create_project\n:::{role="user"}\nassistant.user()'), true);
});

test('does not display punctuation-only output from a broken local model', async () => {
    const replies = [
        { success: true, data: { content: '????????' } },
        { success: true, data: { content: '????' } },
        { success: true, data: { content: '?' } }
    ];
    const result = await runAgentLoop({
        request: { prompt: 'Você sabe programar?' },
        chat: async () => replies.shift(),
        executeActions: async () => []
    });
    assert.match(result.content, /não está respondendo|resposta inválida/);
    assert.doesNotMatch(result.content, /^\?+$/);
});

test('retries when a correction response delegates commands to the user', async () => {
    const replies = [
        { success: true, data: { content: 'Para corrigir, execute estes comandos no PowerShell.' } },
        { success: true, data: { content: 'Corrigido.\n:::NEXA_ACTIONS\n{"actions":[{"kind":"write_file","path":".env","content":"ok"}]}\n:::' } },
        { success: true, data: { content: 'A alteração foi aplicada.' } }
    ];
    const prompts = [];
    const result = await runAgentLoop({
        request: { prompt: 'Prossiga com as correções do projeto.' },
        chat: async request => { prompts.push(request.prompt); return replies.shift(); },
        executeActions: async () => [{ kind: 'write_file', ok: true, details: { path: '.env' } }]
    });
    assert.equal(result.actionResults.length, 1);
    assert.match(prompts[1], /Do not ask the user/);
    assert.match(result.content, /aplicada|Corrigido/);
});

test('retries when no action is produced for a requested correction', async () => {
    let calls = 0;
    const result = await runAgentLoop({
        request: { prompt: 'Corrija o projeto.' },
        chat: async () => { calls++; return { success: true, data: { content: 'Vou criar um backup antes de corrigir.' } }; },
        executeActions: async () => { throw new Error('não deveria executar'); }
    });
    assert.ok(calls > 1, 'should retry when no action is produced');
    assert.match(result.content, /backup|corrigir/);
    assert.deepEqual(result.actionResults, []);
});

test('repairs an invalid local-model action protocol without involving the user', async () => {
    const replies = [
        { success: true, data: { content: ':::NEXA_ACTIONS\n{"actions":[{"kind":"read_file","path":::' } },
        { success: true, data: { content: ':::NEXA_ACTIONS\n{"actions":[{"kind":"read_file","path":"app.js"}]}\n:::' } },
        { success: true, data: { content: 'Arquivo analisado.' } }
    ];
    const prompts = [];
    const result = await runAgentLoop({
        request: { prompt: 'Analise app.js' },
        chat: async request => { prompts.push(request.prompt); return replies.shift(); },
        executeActions: async () => [{ kind: 'read_file', ok: true, details: { path: 'app.js' } }]
    });
    assert.match(prompts[1], /was invalid/);
    assert.equal(result.actionResults[0].ok, true);
    assert.equal(result.content, 'Arquivo analisado.');
});

test('keeps persisted command output bounded', () => {
    const compact = compactActionResults([{ kind: 'run_command', ok: true, details: { stdout: 'x'.repeat(2000), exitCode: 0 } }]);
    assert.equal(compact[0].stdout.length, 1000);
    assert.equal(compact[0].exitCode, 0);
    assert.equal(compact[0].details.stdout.length, 1000);
});
