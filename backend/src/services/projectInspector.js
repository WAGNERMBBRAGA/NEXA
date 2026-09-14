const fs = require('fs');
const path = require('path');

const IGNORED = new Set(['node_modules', 'vendor', '.git', 'dist', 'build', 'coverage', '.next', 'target', '.cache', '__pycache__', 'site-packages', '.venv', 'venv', 'env']);
const SOURCE_EXTENSIONS = new Set(['.js', '.jsx', '.ts', '.tsx', '.php', '.py', '.rs', '.go', '.java', '.cs', '.c', '.cpp', '.h', '.hpp', '.vue', '.svelte']);
const ENTRY_NAMES = new Set(['package.json', 'composer.json', 'cargo.toml', 'pyproject.toml', 'requirements.txt', 'docker-compose.yml', 'compose.yml', 'dockerfile', 'readme.md']);
const MAX_FILES = 2000;
const MAX_TEXT_BYTES = 512 * 1024;

function inspectProject(root) {
    const report = {
        files: 0,
        directories: 0,
        truncated: false,
        topDirectories: [],
        languages: {},
        manifests: [],
        entryFiles: [],
        testFiles: [],
        findings: [],
        notes: [],
        contentInspectedFiles: 0,
        metadataOnlyFiles: 0,
        ignoredDirectories: 0,
        architecture: { ecosystems: [], frameworks: [], entryPoints: [], modules: [], dependencies: [] },
        sourceFiles: 0
    };
    const topDirectories = new Set();
    const dependencies = new Set();
    const frameworks = new Set();

    function addFinding(severity, code, file, line, message) {
        if (report.findings.length >= 100) return;
        report.findings.push({ severity, code, file: file.replace(/\\/g, '/'), line, message });
    }

    function inspectText(fullPath, relative, size) {
        const extension = path.extname(relative).toLowerCase();
        const base = path.basename(relative).toLowerCase();
        if (size > MAX_TEXT_BYTES) {
            report.notes.push({ file: relative.replace(/\\/g, '/'), message: 'Arquivo textual grande; conteúdo não inspecionado integralmente.' });
            return;
        }
        let content;
        try { content = fs.readFileSync(fullPath, 'utf8'); } catch (error) {
            addFinding('error', 'unreadable_file', relative, null, `Não foi possível ler o arquivo: ${error.message}`);
            return;
        }
        report.contentInspectedFiles++;
        if (content.includes('\0')) return;
        if (SOURCE_EXTENSIONS.has(extension) && content.trim() === '') {
            addFinding('warning', 'empty_source_file', relative, 1, 'Arquivo de código vazio.');
        }
        const lines = content.split(/\r?\n/);
        lines.forEach((line, index) => {
            if (/^(<{7}|={7}|>{7})(?:\s|$)/.test(line)) {
                addFinding('error', 'merge_conflict_marker', relative, index + 1, 'Marcador de conflito de merge não resolvido.');
            }
        });
        if (extension === '.json') {
            try { JSON.parse(content); } catch (error) {
                addFinding('error', 'invalid_json', relative, null, `JSON inválido: ${error.message}`);
            }
        }
        if (base === 'requirements.txt') {
            for (const line of lines) {
                const dependency = line.trim().split(/[<>=!~\s\[]/)[0];
                if (dependency && !dependency.startsWith('#')) dependencies.add(dependency);
            }
        }
        if (base === 'package.json') {
            try {
                const manifest = JSON.parse(content);
                Object.keys({ ...(manifest.dependencies || {}), ...(manifest.devDependencies || {}) }).forEach(name => dependencies.add(name));
            } catch {}
        }
    }

    function walk(directory, relativeDirectory = '') {
        if (report.files >= MAX_FILES) { report.truncated = true; return; }
        let entries;
        try { entries = fs.readdirSync(directory, { withFileTypes: true }); } catch (error) {
            addFinding('error', 'unreadable_directory', relativeDirectory || '.', null, `Não foi possível ler a pasta: ${error.message}`);
            return;
        }
        for (const entry of entries) {
            if (report.files >= MAX_FILES) { report.truncated = true; return; }
            const relative = path.join(relativeDirectory, entry.name);
            const fullPath = path.join(directory, entry.name);
            if (entry.isDirectory()) {
                if (IGNORED.has(entry.name) || entry.name.startsWith('.') || /\.(?:dist|egg)-info$/i.test(entry.name)) { report.ignoredDirectories++; continue; }
                report.directories++;
                if (!relativeDirectory) topDirectories.add(entry.name);
                walk(fullPath, relative);
                continue;
            }
            if (!entry.isFile()) continue;
            report.files++;
            const extension = path.extname(entry.name).toLowerCase() || '[sem extensão]';
            if (SOURCE_EXTENSIONS.has(extension)) report.sourceFiles++;
            report.languages[extension] = (report.languages[extension] || 0) + 1;
            const normalized = relative.replace(/\\/g, '/');
            const base = entry.name.toLowerCase();
            if (ENTRY_NAMES.has(base)) report.entryFiles.push(normalized);
            if (/^(?:main|app|server|index|manage|cli)\.(?:py|js|ts|php|rs|go)$/i.test(entry.name)) report.architecture.entryPoints.push(normalized);
            if (['package.json', 'composer.json', 'cargo.toml', 'pyproject.toml', 'requirements.txt'].includes(base)) report.manifests.push(normalized);
            if (/(^|[._-])(test|spec)([._-]|$)/i.test(entry.name) || /(^|[\\/])(tests?|specs?)([\\/]|$)/i.test(relative)) report.testFiles.push(normalized);
            let size = 0;
            try { size = fs.statSync(fullPath).size; } catch {}
            if (SOURCE_EXTENSIONS.has(extension) || ['.json', '.yml', '.yaml', '.toml', '.md', '.txt', '.ini', '.cfg', '.env'].includes(extension) || base === 'requirements.txt') inspectText(fullPath, relative, size);
            else report.metadataOnlyFiles++;
        }
    }

    walk(root);
    report.topDirectories = [...topDirectories].sort();
    report.entryFiles.sort();
    report.manifests.sort();
    report.testFiles = report.testFiles.slice(0, 100).sort();
    report.languages = Object.fromEntries(Object.entries(report.languages).sort((a, b) => b[1] - a[1]).slice(0, 15));
    const extensions = new Set(Object.keys(report.languages));
    if (extensions.has('.py') || report.manifests.some(file => /requirements\.txt|pyproject\.toml/i.test(file))) report.architecture.ecosystems.push('Python');
    if (extensions.has('.js') || extensions.has('.ts') || report.manifests.some(file => /package\.json/i.test(file))) report.architecture.ecosystems.push('Node.js/JavaScript');
    if (extensions.has('.php') || report.manifests.some(file => /composer\.json/i.test(file))) report.architecture.ecosystems.push('PHP');
    if (extensions.has('.rs') || report.manifests.some(file => /cargo\.toml/i.test(file))) report.architecture.ecosystems.push('Rust');
    const knownFrameworks = ['django', 'flask', 'fastapi', 'laravel', 'react', 'vue', 'svelte', 'express', 'pandas', 'scikit-learn', 'joblib'];
    for (const dependency of dependencies) {
        const known = knownFrameworks.find(name => dependency.toLowerCase() === name || dependency.toLowerCase().includes(name));
        if (known) frameworks.add(known);
    }
    report.architecture.frameworks = [...frameworks].sort();
    report.architecture.dependencies = [...dependencies].sort().slice(0, 40);
    report.architecture.entryPoints = [...new Set(report.architecture.entryPoints)].sort();
    report.architecture.modules = report.topDirectories.slice(0, 20);
    if (report.sourceFiles > 0 && report.testFiles.length === 0) {
        addFinding('warning', 'missing_tests', '.', null, 'Nenhum arquivo de teste foi identificado para o código-fonte existente.');
    }
    if (report.sourceFiles > 0 && !report.entryFiles.some(file => /(^|\/)readme\.md$/i.test(file))) {
        addFinding('warning', 'missing_readme', '.', null, 'Nenhum README foi identificado para documentar execução e arquitetura.');
    }
    const manifestGroups = new Map();
    const dependencyGuidePath = path.join(root, 'DEPENDENCIES.md');
    let dependencyGuide = '';
    try { dependencyGuide = fs.existsSync(dependencyGuidePath) ? fs.readFileSync(dependencyGuidePath, 'utf8') : ''; } catch {}
    for (const manifest of report.manifests) {
        const name = path.basename(manifest).toLowerCase();
        if (!manifestGroups.has(name)) manifestGroups.set(name, []);
        manifestGroups.get(name).push(manifest);
    }
    for (const [name, manifests] of manifestGroups) {
        if (manifests.length < 2) continue;
        const contents = manifests.map(file => {
            try { return fs.readFileSync(path.join(root, file), 'utf8').trim(); } catch { return null; }
        });
        const distinct = new Set(contents.filter(content => content !== null));
        const documented = dependencyGuide && manifests.every(file => dependencyGuide.includes(file));
        if (documented) {
            report.notes.push({ file: 'DEPENDENCIES.md', message: `Escopo dos manifestos ${name} documentado.` });
            continue;
        }
        addFinding('warning', distinct.size > 1 ? 'divergent_manifests' : 'duplicate_manifests', manifests.join(', '), null,
            distinct.size > 1 ? `Existem múltiplos ${name} com conteúdos diferentes.` : `O manifesto ${name} está duplicado em mais de uma pasta.`);
    }
    return report;
}

function summarizeInspection(report) {
    const structure = `Estrutura verificada: ${report.files || 0} arquivos e ${report.directories || 0} pastas${report.truncated ? ' (limite de leitura atingido)' : ''}.`;
    const coverage = ` Cobertura: ${report.contentInspectedFiles || 0} arquivo(s) textual(is) lido(s) integralmente, ${report.metadataOnlyFiles || 0} por metadados e ${report.ignoredDirectories || 0} pasta(s) de dependências/cache ignorada(s).`;
    const roots = report.topDirectories && report.topDirectories.length ? ` Pastas principais: ${report.topDirectories.slice(0, 12).join(', ')}.` : '';
    const manifests = report.manifests && report.manifests.length ? ` Manifestos: ${report.manifests.join(', ')}.` : '';
    const architecture = report.architecture || {};
    const architectureLines = [
        architecture.ecosystems && architecture.ecosystems.length ? `- Ecossistema: ${architecture.ecosystems.join(', ')}.` : null,
        architecture.frameworks && architecture.frameworks.length ? `- Frameworks e bibliotecas identificados: ${architecture.frameworks.join(', ')}.` : null,
        architecture.entryPoints && architecture.entryPoints.length ? `- Pontos de entrada: ${architecture.entryPoints.join(', ')}.` : null,
        architecture.modules && architecture.modules.length ? `- Módulos principais: ${architecture.modules.join(', ')}.` : null
    ].filter(Boolean);
    const architectureSummary = architectureLines.length ? `\n\nArquitetura identificada por evidências:\n${architectureLines.join('\n')}` : '';
    const findings = report.findings || [];
    if (!findings.length) return `${structure}${coverage}${roots}${manifests}${architectureSummary}\n\nNenhuma inconsistência estrutural objetiva foi encontrada na inspeção dos arquivos.`;
    const correctionByCode = {
        missing_tests: 'Criar testes automatizados para os pontos de entrada e regras críticas antes de considerar o sistema pronto.',
        missing_readme: 'Documentar instalação, execução, arquitetura e validações em um README.',
        divergent_manifests: 'Definir o escopo de cada manifesto ou consolidar as dependências divergentes para evitar ambientes diferentes.',
        duplicate_manifests: 'Remover a duplicação do manifesto ou documentar claramente por que cada cópia é necessária.',
        invalid_json: 'Corrigir a sintaxe do JSON indicado e executar novamente as validações.',
        merge_conflict_marker: 'Resolver o conflito de merge no arquivo e validar o comportamento resultante.',
        empty_source_file: 'Implementar o arquivo vazio ou removê-lo caso não faça parte da arquitetura.',
        unreadable_file: 'Corrigir a permissão de leitura do arquivo antes de analisá-lo.',
        unreadable_directory: 'Corrigir a permissão da pasta somente se ela fizer parte do código-fonte.'
    };
    const corrections = [...new Set(findings.map(finding => correctionByCode[finding.code]).filter(Boolean))];
    const correctionSummary = corrections.length ? `\n\nCorreções recomendadas a partir desses achados:\n${corrections.map((item, index) => `${index + 1}. ${item}`).join('\n')}` : '';
    return `${structure}${coverage}${roots}${manifests}${architectureSummary}\n\nAchados estruturais comprovados:\n${findings.slice(0, 20).map((finding, index) => `${index + 1}. ${finding.file}${finding.line ? `:${finding.line}` : ''} — ${finding.message}`).join('\n')}${correctionSummary}`;
}

module.exports = { inspectProject, summarizeInspection };
