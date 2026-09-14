const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { structuralFallbackActions } = require('./structuralFallback');

test('does not create generic tests while it documents verified dependency warnings', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-fallback-'));
    try {
        const actions = structuralFallbackActions(root, {
            findings: [{ code: 'missing_tests' }, { code: 'divergent_manifests' }],
            manifests: ['requirements.txt', 'app/requirements.txt'],
            architecture: { ecosystems: ['Python'], entryPoints: ['app.py', 'main.py'] }
        });
        assert.deepEqual(actions.map(action => action.path), ['DEPENDENCIES.md']);
        assert.match(actions[0].content, /requirements\.txt/);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
