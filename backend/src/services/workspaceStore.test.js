const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const test = require('node:test');

test('persists one workspace per project path with shared instructions', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-workspaces-'));
    try {
        const storePath = path.join(__dirname, 'workspaceStore.js');
        const script = `
            const store = require(${JSON.stringify(storePath)});
            const first = store.createWorkspace({ name: 'Academia', projectPath: 'C:/Projetos/Academia' });
            const updated = store.updateWorkspace(first.id, { instructions: 'Use TypeScript e mantenha testes.' });
            const repeated = store.createWorkspace({ name: 'Outro nome', projectPath: 'c:/projetos/academia' });
            const removed = store.deleteWorkspace(first.id);
            console.log(JSON.stringify({ first, updated, repeated, removed, count: store.listWorkspaces().length }));
        `;
        const result = spawnSync(process.execPath, ['-e', script], {
            encoding: 'utf8',
            env: { ...process.env, NEXA_USER_DATA: root }
        });
        assert.equal(result.status, 0, result.stderr);
        const data = JSON.parse(result.stdout.trim());
        assert.equal(data.count, 0);
        assert.equal(data.repeated.id, data.first.id);
        assert.equal(data.updated.instructions, 'Use TypeScript e mantenha testes.');
        assert.equal(data.removed, true);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
