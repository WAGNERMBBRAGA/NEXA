const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { parseActions, executeActions, projectPath, beginTransaction, commitTransaction, rollbackTransaction } = require('./agentActions');

test('parses action blocks and preserves the human response', () => {
    const parsed = parseActions('Vou ajustar.\n:::NEXA_ACTIONS\n{"actions":[{"kind":"write_file","path":"a.txt","content":"ok"}]}\n:::');
    assert.equal(parsed.error, null);
    assert.equal(parsed.actions.length, 1);
    assert.equal(parsed.displayText, 'Vou ajustar.');
});

test('parses action delimiters emitted with literal escaped newlines by GGUF templates', () => {
    const parsed = parseActions(':::NEXA_ACTIONS\\n{"actions":[{"kind":"create_project","path":"novo","files":[{"path":"README.md","content":"# Novo"}]}]}\\n:::');
    assert.equal(parsed.error, null);
    assert.equal(parsed.actions[0].kind, 'create_project');
    assert.equal(parsed.actions[0].files[0].content, '# Novo');
});

test('recovers a compact malformed action wrapper without leaking it into chat', () => {
    const parsed = parseActions(':::NEXA_ACTIONS{"actions":[{"kind":"read_file","path":"README.md"},{"content_type":"markdown"}]}<br>texto indevido<:NEXA_ACTIONS');
    assert.equal(parsed.error, null);
    assert.deepEqual(parsed.actions, [{ kind: 'read_file', path: 'README.md' }]);
    assert.equal(parsed.displayText, '');
});

test('recovers model action JSON with typographic quotes, comments and trailing commas', () => {
    const parsed = parseActions(':::NEXA_ACTIONS\n{“actions”:[{"kind":"read_file", // comentário\n"path":"README.md",}],}\n:::');
    assert.equal(parsed.error, null);
    assert.deepEqual(parsed.actions, [{ kind: 'read_file', path: 'README.md' }]);
});

test('strips markdown fences wrapped around the JSON inside the block markers', () => {
    const fenced = ':::NEXA_ACTIONS\n```json\n{"actions":[{"kind":"create_file","path":"README.md","content":"# Sistema"},{"kind":"create_file","path":"comanda.py","content":"print(\'ok\')"}]}\n```\n:::';
    const parsed = parseActions(fenced);
    assert.equal(parsed.error, null);
    assert.equal(parsed.actions.length, 2);
    assert.equal(parsed.actions[0].path, 'README.md');
    assert.equal(parsed.actions[1].path, 'comanda.py');
});

test('accepts a "commands" list when every entry looks like a NEXA action', () => {
    const parsed = parseActions(':::NEXA_ACTIONS\n{"commands":[{"kind":"create_file","path":"cozinha.py","content":"print(1)"}]}\n:::');
    assert.equal(parsed.error, null);
    assert.equal(parsed.actions.length, 1);
    assert.equal(parsed.actions[0].kind, 'create_file');
    assert.equal(parsed.actions[0].path, 'cozinha.py');
});

test('rejects a "commands" list that is not NEXA-compatible', () => {
    const parsed = parseActions(':::NEXA_ACTIONS\n{"commands":[{"command":"echo hi"}]}\n:::');
    assert.equal(parsed.actions.length, 0);
    assert.match(parsed.error || '', /actions/);
});

test('salvages complete actions from a JSON block truncated at the token limit', () => {
    const truncated = ':::NEXA_ACTIONS\n{"actions":[{"kind":"create_file","path":"README.md","content":"# Sistema"},{"kind":"create_file","path":"menu.json","content":"[1,2,3]"},{"kind":"create_file","path":"comanda.py","content":"#!/usr/bin/env python3\\nimport json\\nprint("';
    const parsed = parseActions(truncated);
    assert.equal(parsed.error, null);
    assert.deepEqual(parsed.actions, [
        { kind: 'create_file', path: 'README.md', content: '# Sistema' },
        { kind: 'create_file', path: 'menu.json', content: '[1,2,3]' }
    ]);
});

test('parses content values that contain real newlines inside the JSON string', () => {
    const resp = ':::NEXA_ACTIONS\n{"actions":[{"kind":"create_file","path":"README.md","content":"# Sistema"},{"kind":"create_file","path":"comanda.py","content":"import json\nmenu = json.load(open(\'menu.json\'))\nprint(1)"}]}\n:::';
    const parsed = parseActions(resp);
    assert.equal(parsed.error, null);
    assert.equal(parsed.actions.length, 2);
    assert.equal(parsed.actions[1].path, 'comanda.py');
    assert.match(parsed.actions[1].content, /import json\nmenu = json\.load/);
});

test('ignores salvaged file mutations without a path field', () => {
    const truncated = ':::NEXA_ACTIONS\n{"actions":[{"kind":"create_file","path":"README.md","content":"# Sistema"},{"kind":"create_file","content":"[1,2,3]"},{"kind":"create_file","path":"comanda.py","content":"print(1"';
    const parsed = parseActions(truncated);
    assert.equal(parsed.error, null);
    assert.deepEqual(parsed.actions, [
        { kind: 'create_file', path: 'README.md', content: '# Sistema' }
    ]);
});

test('writes only inside the selected project', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        const results = await executeActions(root, [{ kind: 'write_file', path: 'src/main.txt', content: 'NEXA' }]);
        assert.equal(results[0].ok, true);
        assert.equal(fs.readFileSync(path.join(root, 'src', 'main.txt'), 'utf8'), 'NEXA');
        assert.equal(fs.readdirSync(path.join(root, 'src')).some(name => name.endsWith('.tmp')), false);
        assert.throws(() => projectPath(root, '../outside.txt'));
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('creates a new file but never overwrites an existing file through create_file', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        const created = await executeActions(root, [{ kind: 'create_file', path: 'tests/test_app.py', content: 'def test_ok():\n    assert True\n' }]);
        assert.equal(created[0].ok, true);
        const blocked = await executeActions(root, [{ kind: 'create_file', path: 'tests/test_app.py', content: 'changed' }]);
        assert.equal(blocked[0].ok, false);
        assert.equal(fs.readFileSync(path.join(root, 'tests', 'test_app.py'), 'utf8'), 'def test_ok():\n    assert True\n');
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('reads only a regular file inside the selected project', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        fs.writeFileSync(path.join(root, 'source.txt'), 'conteúdo do projeto', 'utf8');
        const results = await executeActions(root, [{ kind: 'read_file', path: 'source.txt' }]);
        assert.equal(results[0].ok, true);
        assert.equal(results[0].details.content, 'conteúdo do projeto');
        const blocked = await executeActions(root, [{ kind: 'read_file', path: '../outside.txt' }]);
        assert.equal(blocked[0].ok, false);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('lists project files without traversing ignored folders', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        fs.mkdirSync(path.join(root, 'src'));
        fs.mkdirSync(path.join(root, 'node_modules'));
        fs.writeFileSync(path.join(root, 'src', 'app.js'), 'export {}');
        fs.writeFileSync(path.join(root, 'node_modules', 'hidden.js'), '');
        const results = await executeActions(root, [{ kind: 'list_files' }]);
        assert.equal(results[0].ok, true);
        assert.deepEqual(results[0].details.files, ['src/app.js']);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('searches project text with verified file and line locations', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        fs.mkdirSync(path.join(root, 'src'));
        fs.writeFileSync(path.join(root, 'src', 'service.js'), 'const status = "ready";\nthrow new Error("falha real");', 'utf8');
        const results = await executeActions(root, [{ kind: 'search_project', query: 'falha real' }]);
        assert.equal(results[0].ok, true);
        assert.deepEqual(results[0].details.matches, [{ path: 'src/service.js', line: 2, text: 'throw new Error("falha real");' }]);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('replaces one verified text occurrence without rewriting unrelated content', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        fs.mkdirSync(path.join(root, 'src'));
        const target = path.join(root, 'src', 'app.js');
        fs.writeFileSync(target, 'const before = 1;\nconst defect = true;\nconst after = 2;\n', 'utf8');
        const results = await executeActions(root, [
            { kind: 'read_file', path: 'src/app.js' },
            { kind: 'replace_text', path: 'src/app.js', oldText: 'const defect = true;', newText: 'const defect = false;' }
        ]);
        assert.equal(results[1].ok, true);
        assert.match(results[1].details.beforePreview, /defect = true/);
        assert.match(results[1].details.preview, /defect = false/);
        assert.equal(fs.readFileSync(target, 'utf8'), 'const before = 1;\nconst defect = false;\nconst after = 2;\n');
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('rolls back changed and newly created files when validation fails', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        fs.writeFileSync(path.join(root, 'existing.js'), 'const stable = true;\n', 'utf8');
        beginTransaction(root);
        await executeActions(root, [
            { kind: 'write_file', path: 'existing.js', content: 'const stable = false;\n' },
            { kind: 'write_file', path: 'created.js', content: 'broken' }
        ]);
        const rollback = rollbackTransaction(root);
        assert.equal(rollback.rolledBack, true);
        assert.equal(fs.readFileSync(path.join(root, 'existing.js'), 'utf8'), 'const stable = true;\n');
        assert.equal(fs.existsSync(path.join(root, 'created.js')), false);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('commits a validated transaction without restoring its files', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        fs.writeFileSync(path.join(root, 'app.js'), 'before', 'utf8');
        beginTransaction(root);
        await executeActions(root, [{ kind: 'write_file', path: 'app.js', content: 'after' }]);
        const committed = commitTransaction(root);
        assert.deepEqual(committed, { committed: true, files: 1 });
        assert.equal(fs.readFileSync(path.join(root, 'app.js'), 'utf8'), 'after');
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('documents local and Docker modes without overwriting existing documentation', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        const created = await executeActions(root, [{ kind: 'document_development_modes' }]);
        assert.equal(created[0].ok, true);
        assert.equal(created[0].details.created, true);
        const target = path.join(root, 'DEVELOPMENT.md');
        const original = fs.readFileSync(target, 'utf8');
        assert.match(original, /SQLite/);
        assert.match(original, /Docker/);
        assert.match(original, /MySQL/);

        const preserved = await executeActions(root, [{ kind: 'document_development_modes' }]);
        assert.equal(preserved[0].details.created, false);
        assert.equal(fs.readFileSync(target, 'utf8'), original);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('rejects commands outside the development policy', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-agent-'));
    try {
        const results = await executeActions(root, [{ kind: 'run_command', command: ['powershell.exe', '-Command', 'x'] }]);
        assert.equal(results[0].ok, false);
        assert.match(results[0].error, /política/);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
