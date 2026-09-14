const assert = require('node:assert/strict');
const test = require('node:test');
const { buildConversationHistory, apiChatPayload, readStream, shouldUseNativeTools, agentToolsFor } = require('./aiService');

test('uses native tools for project work whenever the local server declares support', () => {
    assert.equal(shouldUseNativeTools({ nativeTools: true }, 2048, false), false);
    assert.equal(shouldUseNativeTools({ nativeTools: true }, 2048, true), true);
    assert.equal(shouldUseNativeTools({ nativeTools: true }, 8192, true), true);
    assert.equal(shouldUseNativeTools({ nativeTools: true }, 16384, true), true);
});

test('sends only the tools needed by a local execution step', () => {
    const names = agentToolsFor(['read_file', 'replace_text']).map(tool => tool.function.name);
    assert.deepEqual(names, ['read_file', 'replace_text']);
});

test('keeps verified assistant work and user intent in the next model request', () => {
    const history = buildConversationHistory([
        { role: 'user', content: 'Analise o projeto.' },
        { role: 'assistant', content: 'Encontrei dois problemas verificados: testes ausentes e manifestos divergentes.' },
        { role: 'user', content: 'Prossiga com as correções.' },
        { role: 'assistant', content: 'Criei tests/test_entry_points.py e DEPENDENCIES.md; a revalidação passou.' }
    ], 4000);

    assert.deepEqual(history.map(item => item.role), ['user', 'assistant', 'user', 'assistant']);
    assert.match(history[3].content, /DEPENDENCIES\.md/);
});

test('uses the same native tool contract for compatible API providers', () => {
    const payload = apiChatPayload({ model: 'any-model' }, [{ role: 'user', content: 'Analise o projeto' }], 0.2, 512);
    assert.equal(payload.model, 'any-model');
    assert.equal(payload.tool_choice, 'auto');
    assert.ok(payload.tools.some(tool => tool.function.name === 'read_file'));
    assert.ok(payload.tools.some(tool => tool.function.name === 'write_file'));
});

test('turns OpenAI-compatible SSE deltas into streamed text', async () => {
    const stream = new ReadableStream({ start(controller) {
        controller.enqueue(new TextEncoder().encode('data: {"choices":[{"delta":{"content":"Olá"}}]}\n\ndata: {"choices":[{"delta":{"content":" mundo"}}]}\n\ndata: [DONE]\n\n'));
        controller.close();
    }});
    const received = [];
    const content = await readStream({ body: stream }, token => received.push(token));
    assert.equal(content, 'Olá mundo');
    assert.deepEqual(received, ['Olá', ' mundo']);
});
