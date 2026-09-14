const assert = require('node:assert/strict');
const test = require('node:test');
const { getEnabledSkillsPrompts } = require('./skillsService');

test('loads the skills service and allows an empty conversation selection', () => {
    assert.equal(getEnabledSkillsPrompts(600, []), '');
});
