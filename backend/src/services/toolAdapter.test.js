const test = require('node:test');
const assert = require('node:assert/strict');
const { nativeActions, textActions, modelProfile } = require('./toolAdapter');
const { structuredToolInstructions, contextBudget } = require('./aiService');

test('normalizes OpenAI tool_calls independent of model name', () => {
    const actions = nativeActions({ tool_calls: [{ function: { name: 'read_file', arguments: '{"path":"src/app.js"}' } }] });
    assert.deepEqual(actions, [{ kind: 'read_file', path: 'src/app.js' }]);
});

test('normalizes legacy function_call', () => {
    const actions = nativeActions({ function_call: { name: 'run_command', arguments: '{"command":["npm","test"]}' } });
    assert.deepEqual(actions, [{ kind: 'run_command', command: ['npm', 'test'] }]);
});

test('normalizes XML tool calls from local model templates', () => {
    const actions = textActions('<tool_call><function=read_file><parameter=path>src/main.rs</parameter></function></tool_call>');
    assert.deepEqual(actions, [{ kind: 'read_file', path: 'src/main.rs' }]);
});

test('normalizes Qwen JSON tool calls embedded in a llama.cpp template', () => {
    const actions = textActions('<tool_call>{"name":"read_file","arguments":{"path":"src/app.js"}}</tool_call>');
    assert.deepEqual(actions, [{ kind: 'read_file', path: 'src/app.js' }]);
});

test('normalizes a fenced Qwen function object returned by a local model', () => {
    assert.deepEqual(textActions('```json\n{"name":"inspect_project","arguments":{}}\n```'), [{ kind: 'inspect_project' }]);
});

test('normalizes structure discovery from native tool calls', () => {
    const actions = nativeActions({ tool_calls: [{ function: { name: 'list_files', arguments: '{"path":"src"}' } }] });
    assert.deepEqual(actions, [{ kind: 'list_files', path: 'src' }]);
});

test('salvages code fences with file headers into create_file actions', () => {
    const actions = textActions(`I'll carefully read the existing files.

\`\`\`python
# kitchen.py
from dataclasses import dataclass

@dataclass
class Order:
    table: int
    items: list

def prepare_order(order: Order) -> None:
    order.status = "cooking"
\`\`\`

\`\`\`python
# src/main.py
def main():
    print("ok")
\`\`\`
`);
    assert.ok(actions.length >= 2, JSON.stringify(actions));
    const kitchen = actions.find(a => a.path === 'kitchen.py');
    const main = actions.find(a => a.path === 'src/main.py');
    assert.equal(kitchen.kind, 'create_file');
    assert.match(kitchen.content, /prepare_order/);
    assert.equal(main.kind, 'create_file');
    assert.match(main.content, /def main/);
});

test('ignores code fences without a file header comment', () => {
    assert.deepEqual(textActions('```python\nprint("hello")\n```'), []);
    assert.deepEqual(textActions('```json\n{"ok":true}\n```'), []);
});

test('does not duplicate the same file from repeated fences', () => {
    const actions = textActions('```python\n# app.py\nx = 1\n```\n```python\n# app.py\ny = 2\n```');
    assert.equal(actions.filter(a => a.path === 'app.py').length, 1);
});

test('creates a model-neutral capability profile', () => {
    assert.deepEqual(modelProfile('D:/models/any-model.gguf', { nativeTools: true, supportsSystem: false }), {
        id: 'any-model.gguf', nativeTools: true, supportsSystem: false, mode: 'ferramentas-nativas'
    });
});

test('provides an executable fallback protocol for models without native tools', () => {
    const instructions = structuredToolInstructions();
    assert.match(instructions, /:::NEXA_ACTIONS/);
    assert.match(instructions, /list_files/);
    assert.match(instructions, /write_file/);
});

test('adapts prompt and response budgets to the active model context window', () => {
    assert.deepEqual(contextBudget(2048, true), { contextSize: 2048, responseTokens: 368, promptTokens: 1616 });
    const large = contextBudget(32768, true);
    assert.equal(large.responseTokens, 1024);
    assert.equal(large.promptTokens, 31680);
});
