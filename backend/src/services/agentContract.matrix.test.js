const test = require('node:test');
const assert = require('node:assert/strict');
const { nativeActions, textActions } = require('./toolAdapter');

// Matriz de compatibilidade do contrato entre modelos e executor. Cada caso
// simula uma resposta de API OpenAI, llama.cpp/Qwen ou template serializado.
// Ela nunca cria arquivos de projetos dos usuários.
const kinds = ['inspect_project', 'audit_project', 'index_project', 'list_files', 'search_project', 'read_file', 'create_file', 'write_file', 'replace_text', 'run_command'];
const paths = Array.from({ length: 20 }, (_, index) => `src/module-${index + 1}/arquivo-${index + 1}.js`);

function argumentsFor(kind, path) {
    return kind === 'run_command' ? { command: ['node', '--check', path] } : { path };
}

for (const kind of kinds) {
    for (const path of paths) {
        const args = argumentsFor(kind, path);
        const expected = { kind, ...args };
        const encoded = JSON.stringify(args);
        test(`nativo ${kind} ${path}`, () => assert.deepEqual(nativeActions({ tool_calls: [{ function: { name: kind, arguments: encoded } }] }), [expected]));
        test(`legado ${kind} ${path}`, () => assert.deepEqual(nativeActions({ function_call: { name: kind, arguments: encoded } }), [expected]));
        test(`xml ${kind} ${path}`, () => {
            const parameter = kind === 'run_command' ? `<parameter=command>${JSON.stringify(args.command)}</parameter>` : `<parameter=path>${path}</parameter>`;
            assert.deepEqual(textActions(`<tool_call><function=${kind}>${parameter}</function></tool_call>`), [expected]);
        });
        test(`qwen ${kind} ${path}`, () => assert.deepEqual(textActions(`<tool_call>{"name":"${kind}","arguments":${encoded}}</tool_call>`), [expected]));
        test(`json ${kind} ${path}`, () => assert.deepEqual(textActions('```json\n' + JSON.stringify({ name: kind, arguments: args }) + '\n```'), [expected]));
    }
}
