const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { refreshIndex, searchIndex } = require('./projectIndex');

test('reuses unchanged files and refreshes only modified project content', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-index-'));
    try {
        const target = path.join(root, 'service.js');
        fs.writeFileSync(target, 'const state = "ready";\n', 'utf8');
        const first = refreshIndex(root);
        assert.equal(first.updated, 1);
        const second = refreshIndex(root);
        assert.equal(second.reused, 1);
        assert.equal(searchIndex(root, 'state ready').matches[0].path, 'service.js');

        await new Promise(resolve => setTimeout(resolve, 10));
        fs.writeFileSync(target, 'const state = "completed";\n', 'utf8');
        const result = searchIndex(root, 'state completed');
        assert.equal(result.index.updated, 1);
        assert.equal(result.matches[0].line, 1);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
