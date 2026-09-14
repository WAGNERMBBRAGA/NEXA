const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { creationBriefAction } = require('./creationFallback');

test('creates a verifiable initial project brief only when missing', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-creation-'));
    try {
        const actions = creationBriefAction(root, 'Criar sistema para academias', { requirements: ['Criar sistema para academias', 'Ter alunos e planos'] });
        assert.equal(actions.length, 1);
        assert.equal(actions[0].kind, 'create_file');
        assert.equal(actions[0].path, 'NEXA_PROJECT_BRIEF.md');
        assert.match(actions[0].content, /Ter alunos e planos/);
        fs.writeFileSync(path.join(root, 'NEXA_PROJECT_BRIEF.md'), actions[0].content);
        assert.deepEqual(creationBriefAction(root, 'Outra solicitação'), [{ kind: 'inspect_project' }]);
        assert.deepEqual(creationBriefAction(root, 'Outra solicitação', {
            completed: ['Estrutura do projeto inspecionada']
        }), [{ kind: 'audit_project' }]);
        assert.deepEqual(creationBriefAction(root, 'Outra solicitação', {
            completed: ['Estrutura do projeto inspecionada', 'Build e testes verificados']
        }), []);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
