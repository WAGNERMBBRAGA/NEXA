const fs = require('fs');
const path = require('path');

function resolveProjectPath(storedPath) {
    if (!storedPath) return null;
    if (fs.existsSync(storedPath)) return storedPath;
    try {
        const parts = storedPath.split(/[\\/]/);
        let current = parts[0] || parts[1] || '';
        for (let i = 1; i < parts.length; i++) {
            const part = parts[i];
            let next = path.join(current, part);
            if (fs.existsSync(next)) {
                current = next;
                continue;
            }
            let found = false;
            try {
                const entries = fs.readdirSync(current, { withFileTypes: true });
                const cleanPart = part.normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/\ufffd/g, '').toLowerCase();
                for (const e of entries) {
                    const cleanEntry = e.name.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();
                    if (cleanEntry === cleanPart || e.name.toLowerCase() === part.toLowerCase() || cleanEntry.startsWith(cleanPart.substring(0, 6))) {
                        current = path.join(current, e.name);
                        found = true;
                        break;
                    }
                }
            } catch {}
            if (!found) return null;
        }
        return fs.existsSync(current) ? current : null;
    } catch {
        return null;
    }
}

const IGNORE_DIRS = new Set([
    'node_modules', '.git', '__pycache__', 'dist', 'build',
    '.next', '.nuxt', 'venv', '.venv', 'env', '.env',
    'target', 'bin', 'obj', '.vscode', '.idea', 'coverage', '__pycache__', 'site-packages',
    '.cache', '.sass-cache', 'vendor', 'packages'
]);

const IGNORE_FILES = new Set([
    '.DS_Store', 'Thumbs.db', 'desktop.ini', '.env.local',
    '.env.production', 'package-lock.json', 'yarn.lock',
    'pnpm-lock.yaml', 'composer.lock', 'Cargo.lock'
]);

const BINARY_EXTS = new Set([
    '.png', '.jpg', '.jpeg', '.gif', '.bmp', '.ico', '.svg',
    '.mp3', '.mp4', '.wav', '.avi', '.mov',
    '.zip', '.tar', '.gz', '.rar', '.7z',
    '.exe', '.dll', '.so', '.dylib',
    '.woff', '.woff2', '.ttf', '.eot',
    '.pdf', '.doc', '.docx', '.xls', '.xlsx'
]);

const LANG_MAP = {
    '.js': 'JavaScript', '.jsx': 'React JSX', '.ts': 'TypeScript', '.tsx': 'React TSX',
    '.py': 'Python', '.pyw': 'Python',
    '.html': 'HTML', '.htm': 'HTML', '.css': 'CSS', '.scss': 'SCSS', '.sass': 'Sass',
    '.json': 'JSON', '.yaml': 'YAML', '.yml': 'YAML', '.toml': 'TOML',
    '.md': 'Markdown', '.mdx': 'MDX',
    '.java': 'Java', '.kt': 'Kotlin', '.scala': 'Scala',
    '.c': 'C', '.cpp': 'C++', '.h': 'C/C++ Header', '.hpp': 'C++ Header',
    '.cs': 'C#', '.fs': 'F#',
    '.go': 'Go', '.rs': 'Rust', '.rb': 'Ruby', '.php': 'PHP',
    '.swift': 'Swift', '.m': 'Objective-C',
    '.sh': 'Shell', '.bash': 'Bash', '.ps1': 'PowerShell',
    '.sql': 'SQL', '.graphql': 'GraphQL', '.gql': 'GraphQL',
    '.vue': 'Vue', '.svelte': 'Svelte',
    '.dockerfile': 'Docker', '.dockerignore': 'Docker',
    '.txt': 'Text', '.csv': 'CSV',
    '.r': 'R', '.R': 'R', '.lua': 'Lua', '.dart': 'Dart',
    '.ex': 'Elixir', '.exs': 'Elixir', '.erl': 'Erlang',
    '.hs': 'Haskell', '.ml': 'OCaml',
    '.xml': 'XML', '.svg': 'SVG'
};

function getLanguage(filePath) {
    const ext = path.extname(filePath).toLowerCase();
    const base = path.basename(filePath).toLowerCase();
    if (base === 'dockerfile') return 'Docker';
    if (base === 'makefile') return 'Make';
    if (base === 'cmakelists.txt') return 'CMake';
    return LANG_MAP[ext] || ext.toUpperCase().replace('.', '') || 'Unknown';
}

function getFileIcon(filePath) {
    const ext = path.extname(filePath).toLowerCase();
    const base = path.basename(filePath).toLowerCase();
    if (base === 'package.json') return '\u{1F4E6}';
    if (base === 'readme.md') return '\u{1F4DD}';
    if (base === 'dockerfile') return '\u{1F433}';
    if (base === 'makefile') return '\u{2699}\u{FE0F}';
    if (base === '.gitignore') return '\u{1F5C4}\u{FE0F}';
    if (ext === '.js' || ext === '.jsx') return '\u{1F7E1}';
    if (ext === '.ts' || ext === '.tsx') return '\u{1F535}';
    if (ext === '.py') return '\u{1F40D}';
    if (ext === '.html' || ext === '.htm') return '\u{1F310}';
    if (ext === '.css' || ext === '.scss') return '\u{1F3A8}';
    if (ext === '.json') return '\u{1F4CB}';
    if (ext === '.md' || ext === '.mdx') return '\u{1F4DD}';
    if (ext === '.java') return '\u{2615}';
    if (ext === '.go') return '\u{1F48E}';
    if (ext === '.rs') return '\u{1F980}';
    if (ext === '.rb') return '\u{1F353}';
    if (ext === '.php') return '\u{1F354}';
    if (ext === '.sql') return '\u{1F5C4}\u{FE0F}';
    if (ext === '.yml' || ext === '.yaml') return '\u{2699}\u{FE0F}';
    if (ext === '.sh' || ext === '.bash') return '\u{1F4BB}';
    if (BINARY_EXTS.has(ext)) return '\u{1F4C4}';
    return '\u{1F4C4}';
}

function shouldIgnoreDir(dirName) {
    return IGNORE_DIRS.has(dirName) || dirName.startsWith('.') || /\.(?:dist|egg)-info$/i.test(dirName);
}

function shouldIgnoreFile(fileName) {
    if (IGNORE_FILES.has(fileName)) return true;
    if (fileName.endsWith('.min.js') || fileName.endsWith('.min.css')) return true;
    return false;
}

function isBinary(filePath) {
    const ext = path.extname(filePath).toLowerCase();
    return BINARY_EXTS.has(ext);
}

function scanFolder(folderPath, depth = 0, maxDepth = 10) {
    if (depth > maxDepth) return [];
    const entries = [];
    let items;
    try {
        items = fs.readdirSync(folderPath, { withFileTypes: true });
    } catch {
        return [];
    }

    const dirs = [];
    const files = [];

    for (const item of items) {
        if (item.name.startsWith('.') && item.name !== '.env.example' && item.name !== '.gitignore') continue;
        if (item.isDirectory()) {
            if (!shouldIgnoreDir(item.name)) dirs.push(item);
        } else if (item.isFile()) {
            if (!shouldIgnoreFile(item.name)) files.push(item);
        }
    }

    dirs.sort((a, b) => a.name.localeCompare(b.name));
    files.sort((a, b) => a.name.localeCompare(b.name));

    for (const dir of dirs) {
        const dirPath = path.join(folderPath, dir.name);
        const children = scanFolder(dirPath, depth + 1, maxDepth);
        entries.push({
            name: dir.name,
            type: 'directory',
            path: dirPath,
            children
        });
    }

    for (const file of files) {
        const filePath = path.join(folderPath, file.name);
        let size = 0;
        try { size = fs.statSync(filePath).size; } catch {}
        entries.push({
            name: file.name,
            type: 'file',
            path: filePath,
            size,
            language: getLanguage(file.name),
            icon: getFileIcon(file.name),
            binary: isBinary(file.name)
        });
    }

    return entries;
}

function getProjectStats(folderPath) {
    const stats = { totalFiles: 0, totalDirs: 0, totalSize: 0, languages: {}, largeFiles: 0 };

    function walk(dir) {
        let items;
        try { items = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
        for (const item of items) {
            if (item.isDirectory()) {
                if (!shouldIgnoreDir(item.name)) {
                    stats.totalDirs++;
                    walk(path.join(dir, item.name));
                }
            } else if (item.isFile() && !shouldIgnoreFile(item.name)) {
                stats.totalFiles++;
                const fp = path.join(dir, item.name);
                const ext = path.extname(item.name).toLowerCase();
                const lang = getLanguage(item.name);
                try {
                    const s = fs.statSync(fp);
                    stats.totalSize += s.size;
                    if (s.size > 1024 * 1024) stats.largeFiles++;
                } catch {}
                if (lang && lang !== 'Unknown') {
                    stats.languages[lang] = (stats.languages[lang] || 0) + 1;
                }
            }
        }
    }
    walk(folderPath);
    stats.totalSizeMB = (stats.totalSize / (1024 * 1024)).toFixed(2);
    return stats;
}

function readFileContent(filePath) {
    if (isBinary(filePath)) return { content: '[Binary file]', binary: true };
    try {
        const content = fs.readFileSync(filePath, 'utf-8');
        return { content, binary: false };
    } catch (e) {
        return { content: `[Error reading file: ${e.message}]`, binary: false, error: true };
    }
}

function writeFileContent(filePath, content) {
    const dir = path.dirname(filePath);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(filePath, content, 'utf-8');
    return { success: true };
}

function createProject(folderPath, files) {
    if (!fs.existsSync(folderPath)) fs.mkdirSync(folderPath, { recursive: true });
    const created = [];
    for (const file of files) {
        const filePath = path.join(folderPath, file.path || file.name);
        const dir = path.dirname(filePath);
        if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
        fs.writeFileSync(filePath, file.content || '', 'utf-8');
        created.push(filePath);
    }
    return { success: true, created };
}

function listAllFiles(folderPath) {
    const files = [];
    function walk(dir, rel) {
        let items;
        try { items = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
        for (const item of items) {
            if (item.isDirectory()) {
                if (!shouldIgnoreDir(item.name)) walk(path.join(dir, item.name), path.join(rel, item.name));
            } else if (item.isFile() && !shouldIgnoreFile(item.name)) {
                const fp = path.join(dir, item.name);
                if (!isBinary(item.name)) {
                    files.push({
                        path: path.join(rel, item.name),
                        fullPath: fp,
                        language: getLanguage(item.name)
                    });
                }
            }
        }
    }
    walk(folderPath, '');
    return files;
}

function readMultipleFiles(folderPath, filePatterns) {
    const allFiles = listAllFiles(folderPath);
    const results = [];
    const limit = 50;
    let count = 0;

    for (const file of allFiles) {
        if (count >= limit) break;
        if (filePatterns && filePatterns.length > 0) {
            const matches = filePatterns.some(p => file.path.includes(p) || file.language.toLowerCase().includes(p.toLowerCase()));
            if (!matches) continue;
        }
        const { content } = readFileContent(file.fullPath);
        if (content && !content.startsWith('[Error') && !content.startsWith('[Binary')) {
            results.push({ path: file.path, content: content.substring(0, 8000), language: file.language });
            count++;
        }
    }
    return results;
}

function readAllProjectFiles(folderPath, maxTotalChars = 30000) {
    const results = [];
    let totalChars = 0;
    const maxFiles = 30;

    function walk(dir, rel) {
        if (results.length >= maxFiles || totalChars >= maxTotalChars) return;
        let items;
        try { items = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
        for (const item of items) {
            if (results.length >= maxFiles || totalChars >= maxTotalChars) return;
            if (item.isDirectory()) {
                if (!shouldIgnoreDir(item.name)) walk(path.join(dir, item.name), path.join(rel, item.name));
            } else if (item.isFile() && !shouldIgnoreFile(item.name) && !isBinary(item.name)) {
                const fp = path.join(dir, item.name);
                const { content } = readFileContent(fp);
                if (content && !content.startsWith('[Error') && !content.startsWith('[Binary')) {
                    const truncated = content.substring(0, 4000);
                    results.push({ path: path.join(rel, item.name), content: truncated, language: getLanguage(item.name) });
                    totalChars += truncated.length;
                }
            }
        }
    }
    walk(folderPath, '');
    return results;
}

module.exports = {
    scanFolder,
    getProjectStats,
    readFileContent,
    writeFileContent,
    createProject,
    listAllFiles,
    readMultipleFiles,
    readAllProjectFiles,
    getLanguage,
    getFileIcon,
    resolveProjectPath
};
