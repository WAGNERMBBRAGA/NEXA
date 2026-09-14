const test = require('node:test');
const assert = require('node:assert/strict');
const { rankFiles, collectContext } = require('./projectContext');

test('prioritizes files named in the request before unrelated files', () => {
    const files = [
        { path: 'src/cache.js' },
        { path: 'src/auth/login.js' },
        { path: 'src/ui/theme.css' }
    ];
    assert.equal(rankFiles(files, 'Corrija o login e autenticação')[0].path, 'src/auth/login.js');
});

test('keeps context bounded and includes project entry files', () => {
    const files = [
        { path: 'src/large.js', fullPath: 'large', language: 'JavaScript' },
        { path: 'package.json', fullPath: 'package', language: 'JSON' }
    ];
    const service = {
        listAllFiles: () => files,
        readFileContent: file => ({ content: file === 'package' ? '{"name":"demo"}' : 'x'.repeat(500) })
    };
    const context = collectContext(service, 'ignored', 'explique o projeto', 200, 2);
    assert.equal(context[0].path, 'package.json');
    assert.ok(context.reduce((sum, f) => sum + f.content.length, 0) <= 200);
});
