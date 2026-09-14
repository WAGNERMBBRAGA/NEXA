const assert = require('node:assert/strict');
const test = require('node:test');
const { updateWorkPlan } = require('./workPlan');

test('keeps completed work when the user continues the same project task', () => {
    const plan = updateWorkPlan({ objective: 'Corrigir o projeto', completed: ['Estrutura inspecionada'], pending: ['tests ausentes'] }, {
        objective: 'prossiga', completed: ['Arquivos criados'], pending: []
    });
    assert.equal(plan.objective, 'Corrigir o projeto');
    assert.deepEqual(plan.completed, ['Estrutura inspecionada', 'Arquivos criados']);
    assert.equal(plan.status, 'complete');
});

test('keeps a creation plan in progress after its initial verifiable action', () => {
    const plan = updateWorkPlan({
        objective: 'Criar uma plataforma', status: 'in_progress', completed: [],
        requirements: ['Criar uma plataforma'],
        pending: ['Inspecionar a pasta vinculada', 'Criar a base inicial do projeto', 'Validar os arquivos criados']
    }, { objective: 'Criar uma plataforma', completed: ['Arquivos criados'], pending: [] });
    assert.equal(plan.status, 'in_progress');
    assert.deepEqual(plan.pending, [
        'Inspecionar a pasta vinculada',
        'Atualizar o índice contextual',
        'Validar a estrutura existente',
        'Definir o blueprint do sistema',
        'Implementar os módulos e validar cada etapa'
    ]);
    assert.deepEqual(plan.requirements, ['Criar uma plataforma']);
});

test('records the concrete generated base after project files are created', () => {
    const plan = updateWorkPlan({ objective: 'Criar sistema', status: 'in_progress', completed: [], pending: [] }, {
        objective: 'Criar sistema', completed: ['Arquivos criados', 'Validações executadas'], pending: []
    });
    assert.equal(plan.blueprint.id, 'node-service');
    assert.equal(plan.blueprintValidated, true);
});
