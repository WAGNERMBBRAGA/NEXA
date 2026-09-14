const assert = require('node:assert/strict');
const test = require('node:test');
const { isVerifiedStatusQuestion, verifiedChangeSummary } = require('./conversationStatus');

test('answers confirmation questions from verified conversation actions without calling a model', () => {
    assert.equal(isVerifiedStatusQuestion('você corrigiu?'), true);
    const response = verifiedChangeSummary([{ kind: 'create_file', ok: true, details: { path: 'tests/test_entry_points.py' } }]);
    assert.match(response, /Sim\./);
    assert.match(response, /tests\/test_entry_points\.py/);
});
