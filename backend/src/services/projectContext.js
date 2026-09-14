const path = require('path');

const ENTRY_FILES = new Set([
    'package.json', 'cargo.toml', 'pyproject.toml', 'requirements.txt',
    'readme.md', 'dockerfile', 'compose.yml', 'docker-compose.yml'
]);
const STOP_WORDS = new Set(['para', 'com', 'como', 'uma', 'que', 'isso', 'este', 'esta', 'arquivo', 'projeto', 'codigo', 'código', 'teste', 'testes', 'faca', 'faça', 'crie', 'criar']);

function keywords(request) {
    return [...new Set((request || '').toLowerCase().match(/[\p{L}\p{N}_-]{3,}/gu) || [])]
        .filter(word => !STOP_WORDS.has(word));
}

function rankFiles(files, request) {
    const terms = keywords(request);
    return [...files].sort((a, b) => score(b) - score(a) || a.path.localeCompare(b.path));

    function score(file) {
        const normalized = file.path.toLowerCase();
        const name = path.basename(normalized);
        let value = ENTRY_FILES.has(name) ? 20 : 0;
        for (const term of terms) {
            if (name.includes(term)) value += 14;
            else if (normalized.includes(term)) value += 6;
        }
        return value;
    }
}

function collectContext(projectService, root, request, maxChars = 6000, maxFiles = 6) {
    const selected = [];
    let used = 0;
    for (const file of rankFiles(projectService.listAllFiles(root), request)) {
        if (selected.length >= maxFiles || used >= maxChars) break;
        const { content } = projectService.readFileContent(file.fullPath);
        if (!content || content.startsWith('[Error') || content.startsWith('[Binary')) continue;
        const remaining = maxChars - used;
        const clipped = content.slice(0, Math.min(1200, remaining));
        if (!clipped) continue;
        selected.push({ path: file.path, language: file.language, content: clipped });
        used += clipped.length;
    }
    return selected;
}

module.exports = { keywords, rankFiles, collectContext };
