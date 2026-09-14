const test = require('node:test');
const assert = require('node:assert/strict');
const { prepareActions } = require('./actionPlanner');

test('requires a file read before overwriting it', () => {
    const plan = prepareActions([{ kind: 'write_file', path: 'src/app.js', content: 'x' }]);
    assert.equal(plan.allowed.length, 0);
    assert.match(plan.rejected[0].error, /Leia/);
});

test('allows edit after read in the same agent step', () => {
    const plan = prepareActions([{ kind: 'read_file', path: 'src/app.js' }, { kind: 'write_file', path: 'src/app.js', content: 'x' }]);
    assert.equal(plan.allowed.length, 2);
});

test('rejects create_file with empty content so the model regenerates it', () => {
    const plan = prepareActions([{ kind: 'create_file', path: 'restaurant_system/app.py', content: '' }]);
    assert.equal(plan.allowed.length, 0);
    assert.match(plan.rejected[0].error, /vazio/);
});

test('allows create_file with real content', () => {
    const plan = prepareActions([{ kind: 'create_file', path: 'restaurant_system/app.py', content: 'print("ok")' }]);
    assert.equal(plan.allowed.length, 1);
});
