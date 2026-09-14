const fs = require('fs');
const path = require('path');

const IGNORED = new Set(['node_modules', 'vendor', '.git', 'dist', 'build', 'coverage', '.next', 'target', '.cache', '__pycache__', 'site-packages', '.venv', 'venv', 'env']);
const MAX_FILES = 5000;
const MAX_BYTES = 512 * 1024;
const caches = new Map();

function cacheKey(root) {
    return fs.realpathSync(root).toLocaleLowerCase();
}

function refreshIndex(root) {
    const key = cacheKey(root);
    const previous = caches.get(key) || { files: new Map() };
    const files = new Map();
    let scanned = 0;
    let reused = 0;
    let updated = 0;
    let truncated = false;

    function walk(directory) {
        if (scanned >= MAX_FILES) { truncated = true; return; }
        let entries;
        try { entries = fs.readdirSync(directory, { withFileTypes: true }); } catch { return; }
        for (const entry of entries) {
            if (scanned >= MAX_FILES) { truncated = true; return; }
            const fullPath = path.join(directory, entry.name);
            if (entry.isDirectory()) {
                if (!IGNORED.has(entry.name) && !entry.name.startsWith('.') && !/\.(?:dist|egg)-info$/i.test(entry.name)) walk(fullPath);
                continue;
            }
            if (!entry.isFile()) continue;
            let stat;
            try { stat = fs.statSync(fullPath); } catch { continue; }
            if (stat.size > MAX_BYTES) continue;
            scanned++;
            const relative = path.relative(root, fullPath).replace(/\\/g, '/');
            const old = previous.files.get(relative);
            if (old && old.mtimeMs === stat.mtimeMs && old.size === stat.size) {
                files.set(relative, old);
                reused++;
                continue;
            }
            let content;
            try { content = fs.readFileSync(fullPath, 'utf8'); } catch { continue; }
            if (content.includes('\0')) continue;
            const lines = content.split(/\r?\n/).map((text, index) => ({ number: index + 1, text, normalized: text.toLocaleLowerCase() }));
            files.set(relative, { mtimeMs: stat.mtimeMs, size: stat.size, lines });
            updated++;
        }
    }

    walk(root);
    const removed = [...previous.files.keys()].filter(file => !files.has(file)).length;
    caches.set(key, { files });
    return { files: files.size, scanned, reused, updated, removed, truncated };
}

function searchIndex(root, query, limit = 80) {
    const index = refreshIndex(root);
    const cache = caches.get(cacheKey(root));
    const terms = String(query || '').toLocaleLowerCase().split(/\s+/).filter(Boolean);
    const matches = [];
    if (!terms.length) return { query, matches, index, truncated: false };
    for (const [file, record] of cache.files) {
        for (const line of record.lines) {
            if (terms.every(term => line.normalized.includes(term))) {
                matches.push({ path: file, line: line.number, text: line.text.trim().slice(0, 300) });
                if (matches.length >= limit) return { query, matches, index, truncated: true };
            }
        }
    }
    return { query, matches, index, truncated: false };
}

module.exports = { refreshIndex, searchIndex };
