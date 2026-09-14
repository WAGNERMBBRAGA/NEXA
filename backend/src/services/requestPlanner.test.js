const test = require('node:test');
const assert = require('node:assert/strict');
const { planRequest } = require('./requestPlanner');

test('plans a real audit for project diagnostics in Portuguese', () => {
    assert.deepEqual(planRequest('Quais erros tem no projeto?'), {
        actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }],
        mode: 'project_analysis'
    });
});

test('treats a request to inspect every project file as a verified project audit', () => {
    assert.deepEqual(planRequest('analisar o projeto por completo todos os arquivos em busca de erros'), {
        actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }],
        mode: 'project_analysis'
    });
});

test('plans explicit file reads without inventing paths', () => {
    assert.deepEqual(planRequest('Leia backend/composer.json e frontend/package.json'), {
        actions: [
            { kind: 'read_file', path: 'backend/composer.json' },
            { kind: 'read_file', path: 'frontend/package.json' }
        ]
    });
});

test('creates only a fully specified minimal project without model inference', () => {
    assert.deepEqual(planRequest('Crie um projeto mínimo chamado exemplo com o arquivo README.md contendo exatamente: # Exemplo'), {
        actions: [{ kind: 'create_project', path: 'exemplo', files: [{ path: 'README.md', content: '# Exemplo' }] }]
    });
});

test('keeps a language-choice conversation as planning instead of creating files', () => {
    const plan = planRequest('Vamos criar um sistema para academias? Qual linguagem você sugere?', {});
    assert.deepEqual(plan.actions, []);
});

test('plans an explicit replacement as read then write', () => {
    assert.deepEqual(planRequest('Substitua todo o conteúdo do arquivo exemplo/README.md por: # Atualizado'), {
        actions: [
            { kind: 'read_file', path: 'exemplo/README.md' },
            { kind: 'write_file', path: 'exemplo/README.md', content: '# Atualizado' }
        ]
    });
});

test('does not turn ordinary conversation into filesystem work', () => {
    assert.deepEqual(planRequest('Explique como este aplicativo funciona'), { actions: [] });
});

test('treats correction follow-up as a verified audit continuation', () => {
    assert.deepEqual(planRequest('prossiga com as correções'), {
        actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }],
        mode: 'continue_corrections'
    });
});

test('continues the verified project workflow when the user only says prossiga', () => {
    const lastAssistant = { actionResults: [{ kind: 'audit_project', ok: false }] };
    assert.deepEqual(planRequest('prossiga', { lastAssistant }), {
        actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }],
        mode: 'continue_corrections'
    });
});

test('continues a project after a status confirmation with no actions of its own', () => {
    assert.deepEqual(planRequest('prossiga', {
        lastAssistant: { actionResults: [] },
        hasProjectWork: true,
        taskState: { completed: ['Estrutura do projeto inspecionada'] }
    }), {
        actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }],
        mode: 'continue_corrections'
    });
});

test('routes architecture and readiness questions through verified project analysis', () => {
    const expected = {
        actions: [{ kind: 'index_project' }, { kind: 'inspect_project' }, { kind: 'audit_project' }],
        mode: 'project_analysis'
    };
    assert.deepEqual(planRequest('REVISAR PROJETO E IDENTIFICAR A ARQUITETURA'), expected);
    assert.deepEqual(planRequest('O QUE PRECISA SER FEITO PARA O SISTEMA FUNCIONAR PERFEITAMENTE'), expected);
});
