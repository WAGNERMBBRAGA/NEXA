const test = require('node:test');
const assert = require('node:assert/strict');
const { correctionEvidence } = require('./correctionPlanner');

test('turns verified structural warnings into correction work', () => {
    const evidence = correctionEvidence({ findings: [
        { severity: 'warning', code: 'missing_tests', file: '.' },
        { severity: 'warning', code: 'divergent_manifests', file: 'requirements.txt, app/requirements.txt' }
    ] }, { checks: [] });
    assert.deepEqual(evidence.findings.map(item => item.code), ['missing_tests', 'divergent_manifests']);
});

test('keeps unavailable Docker out of automatic source corrections', () => {
    const evidence = correctionEvidence({}, { checks: [
        { name: 'docker_disponivel', ok: false },
        { name: 'python_syntax', ok: false }
    ] });
    assert.deepEqual(evidence.checks.map(item => item.name), ['python_syntax']);
});
