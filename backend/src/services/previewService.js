const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

// Detecta como "rodar" o projeto: interface web estática para embutir num
// iframe, ou um entry point executável (Python/Node) para um terminal real.
function detectPreviewType(root) {
    if (!root || !fs.existsSync(root) || !fs.statSync(root).isDirectory()) {
        return { type: 'none', reason: 'Nenhuma pasta de projeto válida.' };
    }
    const htmlCandidates = ['web/index.html', 'index.html', 'public/index.html', 'src/index.html'];
    for (const rel of htmlCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            return { type: 'web', htmlPath: rel.replace(/\\/g, '/'), entryPath: rel.replace(/\\/g, '/') };
        }
    }
    const pyCandidates = ['src/main.py', 'main.py', 'app.py', 'run.py', 'src/app.py'];
    for (const rel of pyCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            return {
                type: 'python',
                entryPath: rel.replace(/\\/g, '/'),
                command: ['python', rel.replace(/\\/g, '/')],
                reason: 'Aplicativo Python. Clique em Executar para ver a saída real.'
            };
        }
    }
    const jsCandidates = ['src/server.js', 'server.js', 'index.js', 'src/index.js'];
    for (const rel of jsCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            return {
                type: 'node',
                entryPath: rel.replace(/\\/g, '/'),
                command: ['node', rel.replace(/\\/g, '/')],
                reason: 'Aplicativo Node.js. Clique em Executar para ver a saída real.'
            };
        }
    }
    // Projetos PHP/Laravel: servem uma interface web via servidor embutido do
    // PHP. A prévia oferece a página ao vivo (iframe) e o terminal real.
    const phpCandidates = ['public/index.php', 'index.php', 'src/index.php', 'src/main.php'];
    for (const rel of phpCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            const docRoot = rel.startsWith('public/') ? 'public' : '.';
            return {
                type: 'php',
                entryPath: rel.replace(/\\/g, '/'),
                command: ['php', '-S', '127.0.0.1:8000', '-t', docRoot],
                webUrl: 'http://127.0.0.1:8000/',
                reason: 'Aplicativo PHP/Laravel. O NEXA inicia um servidor web local e mostra o sistema ao vivo no canvas.'
            };
        }
    }
    const anyFile = listFilesDeep(root).find(rel => /\.(py|js|ts|php)$/.test(rel));
    if (anyFile) {
        const ext = path.extname(anyFile).slice(1);
        const isPy = ext === 'py';
        return {
            type: isPy ? 'python' : (ext === 'php' ? 'php' : 'node'),
            entryPath: anyFile,
            command: isPy ? ['python', `./${anyFile}`] : (ext === 'php' ? ['php', `./${anyFile}`] : ['node', `./${anyFile}`]),
            reason: `Arquivo ${anyFile} detectado. Executá-lo pode ou não ser o entry point real.`
        };
    }
    return { type: 'none', reason: 'Projeto sem interface web e sem entry point reconhecido.' };
}

// Quando um projeto tem interface web (ex.: landing page estática) mas também
// um entry point executável (Python/Node), a prévia oferece os dois: a página
// embutida e um terminal real. Este helper devolve o lado executável.
function detectExecAlternative(root) {
    if (!root || !fs.existsSync(root) || !fs.statSync(root).isDirectory()) return null;
    const pyCandidates = ['src/main.py', 'main.py', 'app.py', 'run.py', 'src/app.py'];
    for (const rel of pyCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            return {
                type: 'python',
                entryPath: rel.replace(/\\/g, '/'),
                command: ['python', rel.replace(/\\/g, '/')],
                reason: 'Aplicativo Python. Clique em Executar para ver a saída real.'
            };
        }
    }
    const jsCandidates = ['src/server.js', 'server.js', 'index.js', 'src/index.js'];
    for (const rel of jsCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            return {
                type: 'node',
                entryPath: rel.replace(/\\/g, '/'),
                command: ['node', rel.replace(/\\/g, '/')],
                reason: 'Aplicativo Node.js. Clique em Executar para ver a saída real.'
            };
        }
    }
    const phpCandidates = ['public/index.php', 'index.php', 'src/index.php', 'src/main.php'];
    for (const rel of phpCandidates) {
        const file = path.join(root, rel);
        if (fs.existsSync(file) && fs.statSync(file).isFile()) {
            const docRoot = rel.startsWith('public/') ? 'public' : '.';
            return {
                type: 'php',
                entryPath: rel.replace(/\\/g, '/'),
                command: ['php', '-S', '127.0.0.1:8000', '-t', docRoot],
                webUrl: 'http://127.0.0.1:8000/',
                reason: 'Aplicativo PHP/Laravel. Clique em Executar para ver a saída real.'
            };
        }
    }
    return null;
}

function listFilesDeep(dir, base = '', out = []) {
    let entries = [];
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return out; }
    for (const entry of entries) {
        if (entry.name.startsWith('.') || entry.name === 'node_modules' || entry.name === '__pycache__' || entry.name === '.venv' || entry.name === 'venv') continue;
        const rel = base ? base + '/' + entry.name : entry.name;
        if (entry.isDirectory()) listFilesDeep(path.join(dir, entry.name), rel, out);
        else out.push(rel);
    }
    return out;
}

// Executa o entry point do projeto com timeout e captura a saída (stdout+stderr).
// Limitado a 60s e ao prefixo de comandos permitidos pelo NEXA (python/node/etc).
function runProject(root, command, timeoutMs = 60000) {
    if (!Array.isArray(command) || !command.length) {
        return Promise.reject(new Error('Nenhum comando de execução foi definido para este projeto.'));
    }
    const program = command[0];
    if (!/^(python|python3|node|npm|npx|php)$/i.test(program)) {
        return Promise.reject(new Error(`Comando '${program}' não faz parte da política do NEXA.`));
    }
    const args = command.slice(1);
    const cwd = root;
    const timeout = Math.min(Math.max(Number(timeoutMs) || 60000, 2000), 120000);

    return new Promise(resolve => {
        const child = spawn(program, args, { cwd, shell: false, windowsHide: true });
        let stdout = '';
        let stderr = '';
        let timedOut = false;
        const timer = setTimeout(() => {
            timedOut = true;
            child.kill();
        }, timeout);
        child.stdout.on('data', chunk => { stdout += chunk.toString(); });
        child.stderr.on('data', chunk => { stderr += chunk.toString(); });
        child.on('error', error => {
            clearTimeout(timer);
            resolve({ ok: false, timedOut, exitCode: null, stdout, stderr: stderr || error.message });
        });
        child.on('close', code => {
            clearTimeout(timer);
            resolve({ ok: code === 0 && !timedOut, timedOut, exitCode: code, stdout: stdout.slice(0, 20000), stderr: stderr.slice(0, 8000) });
        });
    });
}

function readFileForPreview(root, rel) {
    const target = path.join(root, rel);
    if (!fs.existsSync(target) || !fs.statSync(target).isFile()) return null;
    const content = fs.readFileSync(target, 'utf8');
    return { path: rel.replace(/\\/g, '/'), content: content.slice(0, 100000), bytes: content.length };
}

function listSourceFiles(root) {
    const sizes = { py: 'Python', js: 'JavaScript', ts: 'TypeScript', php: 'PHP', html: 'HTML', css: 'CSS', json: 'JSON', md: 'Markdown' };
    return listFilesDeep(root)
        .filter(rel => /\.(py|js|ts|php|html|css|json|md)$/.test(rel))
        .map(rel => {
            let bytes = 0;
            try { bytes = fs.statSync(path.join(root, rel)).size; } catch {}
            return { path: rel, bytes, language: sizes[path.extname(rel).slice(1)] || 'text' };
        });
}

// Executores em streaming: a saída é emitida em tempo real via onEvent e o
// processo pode ser interrompido a qualquer momento por conversa (stopRun).
const activeRuns = new Map();

const STREAM_MAX_MS = 45000;
const STREAM_ABSOLUTE_MAX_MS = 3600000;
const MAX_EVENT_TEXT = 12000;

function assertAllowedProgram(program) {
    if (!/^(python|python3|node|npm|npx|php)$/i.test(program)) {
        throw new Error(`Comando '${program}' não faz parte da política do NEXA.`);
    }
}

// Inicia (ou reinicia) a execução em streaming para uma conversa/projeto.
// onEvent recebe { type: 'stdout'|'stderr'|'error', text } e depois
// { type: 'exit', code, timedOut } quando o processo encerra.
function streamRun(root, command, onEvent, timeoutMs = STREAM_MAX_MS) {
    if (!Array.isArray(command) || !command.length) {
        throw new Error('Nenhum comando de execução foi definido para este projeto.');
    }
    const program = command[0];
    assertAllowedProgram(program);
    const timeout = Math.min(Math.max(Number(timeoutMs) || STREAM_MAX_MS, 2000), STREAM_ABSOLUTE_MAX_MS);

    const emit = ev => { try { onEvent(ev); } catch {} };

    const previous = activeRuns.get(root);
    if (previous) {
        try { previous.child.kill(); } catch {}
        clearTimeout(previous.timer);
        activeRuns.delete(root);
    }

    const child = spawn(program, command.slice(1), { cwd: root, shell: false, windowsHide: true });
    const entry = { child, closed: false, timedOut: false, timer: null };
    activeRuns.set(root, entry);

    const timer = setTimeout(() => {
        if (entry.closed) return;
        entry.timedOut = true;
        try { child.kill(); } catch {}
        setTimeout(() => {
            if (entry.closed) return;
            entry.closed = true;
            activeRuns.delete(root);
            emit({ type: 'error', text: 'tempo limite atingido e o processo não pôde ser interrompido.' });
        }, 2000);
    }, timeout);
    entry.timer = timer;

    child.stdout.on('data', chunk => emit({ type: 'stdout', text: chunk.toString().slice(0, MAX_EVENT_TEXT) }));
    child.stderr.on('data', chunk => emit({ type: 'stderr', text: chunk.toString().slice(0, MAX_EVENT_TEXT) }));
    child.on('error', error => {
        if (entry.closed) return;
        clearTimeout(timer);
        entry.closed = true;
        activeRuns.delete(root);
        emit({ type: 'error', text: error.message });
    });
    child.on('close', code => {
        if (entry.closed) return;
        clearTimeout(timer);
        entry.closed = true;
        activeRuns.delete(root);
        emit({ type: 'exit', code, timedOut: entry.timedOut });
    });

    return child.pid;
}

// Interrompe a execução ativa de uma conversa/projeto (true se havia processo).
function stopRun(root) {
    const entry = activeRuns.get(root);
    if (!entry) return false;
    if (entry.child && entry.child.pid) {
        try { entry.child.kill(); } catch {}
    }
    return true;
}

function hasActiveRun(root) {
    return activeRuns.has(root);
}

module.exports = { detectPreviewType, detectExecAlternative, runProject, streamRun, stopRun, hasActiveRun, readFileForPreview, listSourceFiles };