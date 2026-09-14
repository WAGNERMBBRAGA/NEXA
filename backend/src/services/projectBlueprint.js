function normalizedText(values) {
    return (values || []).join('\n').toLocaleLowerCase('pt-BR');
}

function buildNodeService(objective, requirements) {
    const projectName = 'nexa-project';
    const description = String(objective || 'Projeto iniciado pelo NEXA.').replace(/[`\r\n]/g, ' ').slice(0, 180);
    const files = [
        {
            path: 'package.json',
            content: JSON.stringify({
                name: projectName,
                version: '0.1.0',
                private: true,
                description,
                scripts: { start: 'node src/server.js' }
            }, null, 2) + '\n'
        },
        {
            path: 'src/store.js',
            content: `'use strict';\n\nfunction createStore() {\n    const members = new Map();\n    return {\n        list: () => [...members.values()],\n        get: id => members.get(id) || null,\n        create: member => {\n            const id = String(member.id || Date.now());\n            const record = { id, name: String(member.name || '').trim(), status: member.status || 'active' };\n            if (!record.name) throw new Error('name é obrigatório');\n            members.set(id, record);\n            return record;\n        }\n    };\n}\n\nmodule.exports = { createStore };\n`
        },
        {
            path: 'src/server.js',
            content: `'use strict';\n\nconst http = require('node:http');\nconst { createStore } = require('./store');\n\nfunction json(response, status, body) {\n    response.writeHead(status, { 'content-type': 'application/json; charset=utf-8' });\n    response.end(JSON.stringify(body));\n}\n\nfunction createApp(store = createStore()) {\n    return http.createServer((request, response) => {\n        if (request.method === 'GET' && request.url === '/health') return json(response, 200, { ok: true });\n        if (request.method === 'GET' && request.url === '/members') return json(response, 200, { data: store.list() });\n        if (request.method === 'POST' && request.url === '/members') {\n            let raw = '';\n            request.on('data', chunk => { raw += chunk; });\n            request.on('end', () => {\n                try { return json(response, 201, { data: store.create(JSON.parse(raw || '{}')) }); }\n                catch (error) { return json(response, 400, { error: error.message }); }\n            });\n            return;\n        }\n        return json(response, 404, { error: 'route not found' });\n    });\n}\n\nif (require.main === module) {\n    const port = Number(process.env.PORT || 3000);\n    createApp().listen(port, () => console.log('NEXA project listening on ' + port));\n}\n\nmodule.exports = { createApp };\n`
        },
        {
            path: 'README.md',
            content: `# Projeto NEXA\n\n## Objetivo\n\n${description}\n\n## Base criada\n\n- API HTTP em Node.js sem dependências externas.\n- Endpoint de saúde: \`GET /health\`.\n- Cadastro inicial em memória: \`GET /members\` e \`POST /members\`.\n- Teste automatizado da regra de membros.\n\n## Próximas implementações\n\n- Persistência, autenticação e interface.\n- Regras específicas registradas nesta conversa.\n`
        }
    ];
    return { id: 'node-service', files, validation: { kind: 'run_command', command: ['node', '--check', 'src/server.js'] } };
}

function createBlueprint(plan = {}) {
    const requirements = Array.isArray(plan.requirements) ? plan.requirements : [];
    const text = normalizedText([plan.objective, ...requirements]);
    // O primeiro blueprint é deliberadamente pequeno, executável e extensível.
    // Ele não finge entregar uma aplicação inteira quando só existe um objetivo.
    const blueprint = buildNodeService(plan.objective, requirements);
    return { ...blueprint, domain: /academia|gym|fitness|aluno|membro/.test(text) ? 'fitness' : 'general' };
}

module.exports = { createBlueprint };
