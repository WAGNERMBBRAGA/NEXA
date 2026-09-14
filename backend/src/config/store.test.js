const assert = require('node:assert/strict');
const test = require('node:test');
const { normalizeOpenAIBaseUrl } = require('./store');

test('normalizes a full OpenAI-compatible chat endpoint into its base URL', () => {
    assert.equal(normalizeOpenAIBaseUrl('https://api.example.test/v1/chat/completions \\'), 'https://api.example.test/v1');
    assert.equal(normalizeOpenAIBaseUrl('https://api.example.test/v1/'), 'https://api.example.test/v1');
});
