const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const test = require('node:test');

test('desktop runtime stores mutable state under NEXA_USER_DATA', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-user-data-'));
    try {
        const script = `
            const paths = require(${JSON.stringify(__filename.replace(/runtimePaths\.test\.js$/, 'runtimePaths.js'))});
            const store = require(${JSON.stringify(path.join(__dirname, 'store.js'))});
            const conversations = require(${JSON.stringify(path.join(__dirname, '..', 'services', 'conversationStore.js'))});
            store.writeConfig({
                provider: 'openai',
                openai: { apiKey: 'segredo-openai' },
                embeddings: { apiKey: 'segredo-embedding' }
            });
            conversations.createConversation('Persistente');
            console.log(JSON.stringify({ config: paths.configFile(), data: paths.dataDir() }));
        `;
        const result = spawnSync(process.execPath, ['-e', script], {
            encoding: 'utf8',
            env: { ...process.env, NEXA_USER_DATA: root }
        });
        assert.equal(result.status, 0, result.stderr);
        const locations = JSON.parse(result.stdout.trim());
        assert.equal(locations.config, path.join(root, 'config.json'));
        assert.equal(locations.data, path.join(root, 'data'));
        assert.equal(fs.existsSync(path.join(root, 'config.json')), true);
        const persisted = fs.readFileSync(path.join(root, 'config.json'), 'utf8');
        assert.equal(persisted.includes('segredo-openai'), false);
        assert.equal(persisted.includes('segredo-embedding'), false);
        assert.equal(fs.readdirSync(path.join(root, 'data', 'conversations')).length, 1);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
