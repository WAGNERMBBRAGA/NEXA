const assert = require('node:assert/strict');
const test = require('node:test');
const { tokenMatches } = require('./apiSecurity');

test('requires the exact desktop API token when configured', () => {
    assert.equal(tokenMatches('secret-value', undefined), false);
    assert.equal(tokenMatches('secret-value', 'wrong-value'), false);
    assert.equal(tokenMatches('secret-value', 'secret-value'), true);
});

test('keeps source development mode available when no token is configured', () => {
    assert.equal(tokenMatches('', undefined), true);
});
