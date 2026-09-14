const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const test = require('node:test');

test('backend standalone persists secrets to data/secrets.json', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-secrets-'));
    try {
        const script = `
            const store = require(${JSON.stringify(path.join(__dirname, 'secretStore.js'))});
            store.save({ openaiApiKey: 'sk-persistido', embeddingsApiKey: '' });
            console.log(JSON.stringify(store.load()));
        `;
        const result = spawnSync(process.execPath, ['-e', script], {
            encoding: 'utf8',
            env: { ...process.env, NEXA_USER_DATA: root }
        });
        assert.equal(result.status, 0, result.stderr);
        const loaded = JSON.parse(result.stdout.trim());
        assert.equal(loaded.openaiApiKey, 'sk-persistido');
        assert.equal(loaded.embeddingsApiKey, '');
        const filePath = path.join(root, 'data', 'secrets.json');
        assert.equal(fs.existsSync(filePath), true);
        assert.equal(fs.readFileSync(filePath, 'utf8').includes('sk-persistido'), true);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('desktop mode does not write data/secrets.json (safeStorage owns it)', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-secrets-desktop-'));
    try {
        const script = `
            const store = require(${JSON.stringify(path.join(__dirname, 'secretStore.js'))});
            store.save({ openaiApiKey: 'sk-desktop' });
            console.log(JSON.stringify({ exists: require('fs').existsSync(require('path').join(${JSON.stringify(root)}, 'data', 'secrets.json')), loaded: store.load() }));
        `;
        const result = spawnSync(process.execPath, ['-e', script], {
            encoding: 'utf8',
            env: { ...process.env, NEXA_USER_DATA: root, NEXA_DESKTOP: '1' }
        });
        assert.equal(result.status, 0, result.stderr);
        const out = JSON.parse(result.stdout.trim());
        assert.equal(out.exists, false);
        assert.deepEqual(out.loaded, { openaiApiKey: '', embeddingsApiKey: '' });
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});