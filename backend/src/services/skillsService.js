const fs = require('fs');
const path = require('path');

const SKILLS_DIR = path.join(__dirname, '..', '..', 'skills');

if (!fs.existsSync(SKILLS_DIR)) {
    fs.mkdirSync(SKILLS_DIR, { recursive: true });
}

function listSkills() {
    const files = fs.readdirSync(SKILLS_DIR).filter(f => f.endsWith('.json'));
    const skills = [];
    for (const file of files) {
        try {
            const data = JSON.parse(fs.readFileSync(path.join(SKILLS_DIR, file), 'utf-8'));
            skills.push({
                id: path.basename(file, '.json'),
                name: data.name || path.basename(file, '.json'),
                description: data.description || '',
                prompt: data.prompt || '',
                enabled: data.enabled !== false,
                category: data.category || 'Geral',
                createdAt: data.createdAt || null,
                updatedAt: data.updatedAt || null
            });
        } catch (e) {
            // skip invalid files
        }
    }
    return skills;
}

function getSkill(id) {
    const filePath = path.join(SKILLS_DIR, id + '.json');
    if (!fs.existsSync(filePath)) return null;
    try {
        return JSON.parse(fs.readFileSync(filePath, 'utf-8'));
    } catch (e) {
        return null;
    }
}

function saveSkill(id, data) {
    const filePath = path.join(SKILLS_DIR, id + '.json');
    const skill = {
        name: data.name || id,
        description: data.description || '',
        prompt: data.prompt || '',
        enabled: data.enabled !== false,
        category: data.category || 'Geral',
        commands: data.commands || [],
        createdAt: data.createdAt || new Date().toISOString(),
        updatedAt: new Date().toISOString()
    };
    fs.writeFileSync(filePath, JSON.stringify(skill, null, 2), 'utf-8');
    return skill;
}

function deleteSkill(id) {
    const filePath = path.join(SKILLS_DIR, id + '.json');
    if (fs.existsSync(filePath)) {
        fs.unlinkSync(filePath);
        return true;
    }
    return false;
}

// Normalização pt->en dos termos mais comuns, para uma pergunta em português
// casar com skills cuja descrição/prompt estão em inglês (ex.: "testes" -> "test",
// "site corporativo" -> "corporate website", "login/senha" -> "auth").
const SKILL_TOKEN_SYNONYMS = {
    // testes / código
    teste: 'test', testes: 'test', test: 'test', tests: 'test', testing: 'test',
    codigo: 'code', código: 'code', code: 'code',
    // núcleo do domínio
    sistema: 'system', projets: 'project', projeto: 'project', projetos: 'project', project: 'project', projects: 'project',
    usuario: 'user', usuarios: 'user', user: 'user', users: 'user',
    criar: 'create', criando: 'create', cria: 'create', crie: 'create', create: 'create', criacao: 'create',
    site: 'site', website: 'site', web: 'site',
    corporativo: 'corporate', corporativa: 'corporate', corporate: 'corporate', institucional: 'corporate', empresa: 'corporate', company: 'corporate', brand: 'corporate', landing: 'landing', landing_page: 'landing', pagina_inicial: 'landing', homepage: 'landing', home: 'landing',
    // entidades de dados
    tarefa: 'task', tarefas: 'task', task: 'task', tasks: 'task',
    nota: 'note', notas: 'note', note: 'note', notes: 'note',
    lembrete: 'reminder', lembretes: 'reminder', reminder: 'reminder', reminders: 'reminder',
    calendario: 'calendar', agenda: 'calendar', calendar: 'calendar',
    dashboard: 'dashboard', painel: 'dashboard',
    venda: 'sales', vendas: 'sales', sales: 'sales', pedidos: 'sales', comandas: 'sales', comanda: 'sales', cardapio: 'sales', menu: 'sales', restaurante: 'sales', restaurant: 'sales', garcom: 'sales', cliente: 'client', clientes: 'client', client: 'client',
    despesa: 'expense', despesas: 'expense', expense: 'expense', custo: 'expense', gasto: 'expense',
    // banco / permissão
    banco: 'database', database: 'database', dados: 'database', db: 'database', schema: 'database', armazenar: 'database', armazenamento: 'database', persistencia: 'database', salvar: 'database', mysql: 'database', sqlite: 'database', sql: 'database', migration: 'database', migrations: 'database',
    login: 'auth', senha: 'auth', autenticacao: 'auth', autenticação: 'auth', auth: 'auth', authentication: 'auth', entrar: 'auth', registrar: 'auth', user_route: 'auth', authcontroller: 'auth',
    // framework / frontend
    frontend: 'frontend', interface: 'frontend', ui: 'frontend', front_end: 'frontend',
    bootstrap: 'bootstrap', boostrap: 'bootstrap',
    tailwind: 'tailwind', tailwindcss: 'tailwind',
    mui: 'mui', material: 'mui', materialui: 'mui', material_ui: 'mui',
    shadcn: 'shadcn', shadcnui: 'shadcn', shadcn_ui: 'shadcn', radix: 'shadcn',
    react: 'react', vue: 'vue', angular: 'angular', svelte: 'svelte', reactnative: 'reactnative', aplicativo: 'reactnative', aplicativo_movel: 'reactnative', aplicativos: 'reactnative', mobile: 'reactnative', celular: 'reactnative', app: 'reactnative',
    componente: 'component', componentes: 'component', component: 'component', components: 'component',
    pagina: 'page', página: 'page', page: 'page', tela: 'page', telas: 'page', screen: 'page', view: 'page', views: 'page', blade: 'page',
    css: 'css', estilos: 'css', estilo: 'css', styles: 'css', tema: 'css', themes: 'css', cores: 'css', color: 'css', cores_tema: 'css',
    // linguagens / backend
    golang: 'golang', go: 'golang',
    laravel: 'laravel', php: 'php', elouquent: 'laravel', eloquent: 'laravel', blade_templates: 'laravel', controllers: 'laravel', controller: 'laravel', rotas: 'route', rota: 'route', routes: 'route', route: 'route', middleware: 'laravel', artisan: 'laravel', migration_php: 'laravel',
    python: 'python', node: 'node', nodejs: 'node', javascript: 'js', js: 'js', typescript: 'js', ts: 'js', express: 'node', flask: 'python', django: 'python',
    backend: 'backend', front_controller: 'backend', servidor: 'backend', server: 'backend', api: 'api', apis: 'api', endpoint: 'api', endpoints: 'api',
    // processo
    testes: 'test', escrever_teste: 'test', escrever_testes: 'test', testear: 'test', testar: 'test', testado: 'test', test_de_unidade: 'test', unit_test: 'test', unit_tests: 'test', teste_unitario: 'test', testes_unitarios: 'test', phpunit: 'test', pytest: 'test', jest: 'test', vitest: 'test', tdd: 'test',
    explicar: 'explain', explicacao: 'explain', explicação: 'explain', explain: 'explain', entender: 'explain', explica: 'explain', explicando: 'explain',
    debugar: 'debug', depurar: 'debug', debug: 'debug', debugando: 'debug', corrigir: 'fix', corrigindo: 'fix', corrige: 'fix', fix: 'fix', correcao: 'fix', erro: 'fix', solving: 'fix', problema: 'fix', problemas: 'fix',
    backup: 'backup', copia: 'backup', restaurar: 'backup', restore: 'backup', restauracao: 'backup', salvamento: 'backup',
    planejar: 'plan', planejamento: 'plan', plan: 'plan', planos: 'plan', plano: 'plan', planejar_trabalho: 'plan', roadmap: 'plan', plano_acao: 'plan', implementar: 'implement', implementation: 'implement', implementacao: 'implement', implementação: 'implement', implemente: 'implement', execute: 'execute', execucao: 'execute',
    especificar: 'spec', especificacao: 'spec', especificação: 'spec', specification: 'spec', spec: 'spec', requisitos: 'spec', requirements: 'spec', requisito: 'spec', ears: 'spec', incose: 'spec',
    lembrar: 'memory', memoria: 'memory', memory: 'memory', memorias: 'memory', persistente: 'memory', persistencia_memoria: 'memory', projeto_memoria: 'memory', estado: 'memory', decisoes: 'memory',
    auditar: 'review', auditoria: 'review', audit: 'review', revisar: 'review', revisor: 'review', revisao: 'review', revisão: 'review', review: 'review', code_review: 'review', codigo_review: 'review', inspecao: 'review', avaliar: 'review', avaliacao: 'review', verificar: 'review', verificado: 'review', verify: 'review', verifiqu: 'review', conferir: 'review',
    publicar: 'publish', publicacao: 'publish', publicação: 'publish', publish: 'publish', publica: 'publish', hospedar: 'publish', hosting: 'publish',
    gerar: 'generate', generate: 'generate', gerando: 'generate', gerador: 'generate',
    construir: 'build', construa: 'build', build: 'build', compilar: 'build', compile: 'build',
    documentar: 'docs', documentacao: 'docs', documentação: 'docs', documentation: 'docs', doc: 'docs', docs: 'docs', wiki: 'docs', manual: 'docs', readme: 'docs', markdown: 'docs', arquivar: 'docs',
    comando: 'command', comandos: 'command', command: 'command', commands: 'command', terminal: 'command', powershell: 'command', executar: 'command', execute: 'command', cli: 'command',
    arquivo: 'file', arquivos: 'file', file: 'file', files: 'file', diretorio: 'file', diretorios: 'file', directory: 'file', pasta: 'file', pastas: 'file', folder: 'file', folders: 'file',
    modulo: 'module', modulos: 'module', module: 'module', modules: 'module', funcionalidade: 'module', funcionalidades: 'module',
    // design
    design: 'design', designer: 'design', estilo: 'design', visual: 'design', direcao: 'design', art_direction: 'design', direcao_de_arte: 'design', identidade: 'design', branding: 'design', marca: 'design', refine: 'design', refinamento: 'design', refinement: 'design', jury: 'design', julgamento: 'design', juri_de_design: 'design',
    imagem: 'image', imagens: 'image', image: 'image', images: 'image', foto: 'image', fotos: 'image', logo: 'image', ilustracao: 'image', ilustração: 'image',
    animacao: 'motion', animacoes: 'motion', animation: 'motion', animations: 'motion', motion: 'motion', movimento: 'motion', transicao: 'motion', transicoes: 'motion', easing: 'motion', easings: 'motion', timing: 'motion', spring: 'motion',
    tipografia: 'typography', typography: 'typography', fontes: 'typography', font: 'typography', espacamento: 'typography', spacing: 'typography', icones: 'typography', icons: 'typography',
    layout: 'layout', grid: 'layout', responsivo: 'layout', responsive: 'layout', breakpoints: 'layout', escalabilidade: 'layout', espaçamento: 'layout',
    seguranca: 'security', segurança: 'security', security: 'security', proteger: 'security', protecao: 'security', proteção: 'security', vulnerabilidade: 'security', csrf: 'security', xss: 'security', desenvolvedor: 'security', development: 'security', developer: 'security',
    acessibilidade: 'accessibility', accessibility: 'accessibility', acessivel: 'accessibility', acessível: 'accessibility', aria: 'accessibility', wcag: 'accessibility',
    slides: 'slides', apresentacao: 'slides', apresentações: 'slides', presentation: 'slides', pptx: 'slides', ppt: 'slides', deck: 'slides', slide: 'slides',
    preview: 'preview', visualizar: 'preview', previsualizar: 'preview', preview_da_interface: 'preview', canvas: 'preview',
    template: 'template', templates: 'template', modelo: 'template', modelos: 'template',
    gerencie: 'manage', gerenciar: 'manage', gerenciamento: 'manage', manage: 'manage', gerencia: 'manage', administrar: 'manage', administracao: 'manage'
};

const SKILL_STOP_WORDS = new Set(['skill', 'skills', 'helper', 'para', 'com', 'como', 'quando', 'use', 'using', 'framework', 'projeto', 'projetos', 'arquivo', 'arquivos', 'codigo', 'código', 'sistema', 'novo', 'nova', 'quero', 'quer', 'pode', 'poderia', 'criar', 'criando', 'cria', 'crie', 'por', 'que', 'favor', 'voce', 'você', 'uma', 'sobre', 'the', 'and', 'for', 'your', 'melhor', 'bom', 'tambem', 'também', 'agora', 'ser', 'porque', 'pois']);

function tokenizeSkillText(text) {
    return [...new Set(String(text || '').toLowerCase().match(/[a-zà-ÿ0-9+#.-]{3,}/g) || [])]
        .map(token => token.replace(/^[\W_]+|[\W_]+$/g, ''))
        .map(token => SKILL_TOKEN_SYNONYMS[token] || token)
        .filter(word => !SKILL_STOP_WORDS.has(word));
}

// Relevância por sobreposição de tokens (palavras do texto × palavras da skill),
// não por substring solta. Retorna a pontuação numérica (0 = irrelevante) para
// permitir ranking; assim "criar um site corporativo" elege a skill de sites
// corporativos em vez de acionar sempre a primeira skill do menu (backup).
function isRelevantToMessage(skill, message) {
    const queryTokens = tokenizeSkillText(message);
    if (queryTokens.length === 0) return 0;
    const sourceTokens = new Set(tokenizeSkillText(`${skill.name} ${skill.description} ${skill.category} ${skill.prompt.slice(0, 450)}`));
    const nameTokens = new Set(tokenizeSkillText(skill.name));
    let score = 0;
    for (const qt of queryTokens) {
        if (sourceTokens.has(qt)) {
            score += nameTokens.has(qt) ? 6 : 3;
            continue;
        }
        for (const st of sourceTokens) {
            if (qt.length >= 4 && st.length >= 4 && (qt.includes(st) || st.includes(qt))) {
                score += 1;
                break;
            }
        }
    }
    return score >= 3 ? score : 0;
}

function getEnabledSkillsPrompts(maxTotalChars = 3000, selectedIds = null, message = '') {
    const selectedIdsSet = Array.isArray(selectedIds) ? new Set(selectedIds) : null;
    let skills = listSkills()
        .filter(s => selectedIdsSet ? selectedIdsSet.has(s.id) : s.enabled);
    if (message) {
        // Ranqueia por relevância antes de encaixar no orçamento de caracteres,
        // para a skill realmente relacionada ao pedido não ficar de fora.
        const scored = skills.map(s => ({ s, score: isRelevantToMessage(s, message) }));
        skills = [];
        for (const item of scored) {
            if (item.score > 0) skills.push({ ...item.s, __relevance__: item.score });
        }
        skills.sort((a, b) => (b.__relevance__ || 0) - (a.__relevance__ || 0));
    }
    if (skills.length === 0) return '';

    let totalChars = 0;
    const selected = [];

    for (const s of skills) {
        const promptText = s.prompt.length > 500 ? s.prompt.substring(0, 500) + '...' : s.prompt;
        if (totalChars + promptText.length > maxTotalChars) break;
        selected.push('## ' + s.name + '\n' + promptText);
        totalChars += promptText.length;
    }

    if (selected.length === 0) return '';

    let result = '\n\n[SKILLS ATIVAS - ' + selected.length + '/' + skills.length + ']\n' + selected.join('\n\n');
    if (selected.length < skills.length) {
        result += '\n\n(Desative skills menos usadas no menu Skills para melhor performance)';
    }
    return result;
}

module.exports = {
    listSkills,
    getSkill,
    saveSkill,
    deleteSkill,
    getEnabledSkillsPrompts
    ,isRelevantToMessage
};
