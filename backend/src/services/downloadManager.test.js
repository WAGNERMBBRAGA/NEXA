const assert = require('node:assert/strict');
const path = require('path');
const test = require('node:test');
const { validateDownloadRequest, getDownloadPath, DOWNLOADS_DIR } = require('./downloadManager');

test('accepts a Hugging Face model and a plain GGUF filename', () => {
    assert.deepEqual(validateDownloadRequest('Qwen/Qwen3.5-4B-GGUF', 'Qwen3.5-4B-Q4_K_M.gguf'), {
        modelId: 'Qwen/Qwen3.5-4B-GGUF',
        filename: 'Qwen3.5-4B-Q4_K_M.gguf'
    });
    assert.equal(getDownloadPath('model.gguf'), path.join(DOWNLOADS_DIR, 'model.gguf'));
});

test('rejects traversal, absolute paths and non-GGUF destinations', () => {
    for (const filename of ['../outside.gguf', '..\\outside.gguf', 'C:\\outside.gguf', 'folder/model.gguf', 'model.exe']) {
        assert.throws(() => validateDownloadRequest('owner/model', filename), /inválido/);
    }
    assert.throws(() => validateDownloadRequest('../owner/model', 'model.gguf'), /Identificador/);
});
