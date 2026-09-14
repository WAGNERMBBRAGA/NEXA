const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createBlueprint } = require('./projectBlueprint');
const { executeActions } = require('./agentActions');

test('creates an executable foundation instead of generic test files', () => {
    const blueprint = createBlueprint({ objective: 'Construir um sistema para academias', requirements: ['alunos e planos'] });
    const paths = blueprint.files.map(file => file.path);
    assert.equal(blueprint.domain, 'fitness');
    assert.ok(paths.includes('src/server.js'));
    assert.ok(paths.includes('src/store.js'));
    assert.equal(paths.some(file => file.startsWith('test/')), false);
    assert.deepEqual(blueprint.validation.command, ['node', '--check', 'src/server.js']);
});

test('writes and validates the generated foundation in an empty selected folder', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-blueprint-'));
    try {
        const blueprint = createBlueprint({ objective: 'Sistema para academias' });
        const writes = await executeActions(root, blueprint.files.map(file => ({ kind: 'create_file', ...file })));
        assert.ok(writes.every(result => result.ok));
        const validation = await executeActions(root, [blueprint.validation]);
        assert.equal(validation[0].ok, true);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
