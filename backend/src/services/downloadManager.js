const fs = require('fs');
const path = require('path');
const https = require('https');

const DOWNLOADS_DIR = process.env.NEXA_MODELS_DIR || 'D:\\models\\huggingface';
const activeDownloads = new Map();

if (!fs.existsSync(DOWNLOADS_DIR)) fs.mkdirSync(DOWNLOADS_DIR, { recursive: true });

function validateDownloadRequest(modelId, filename) {
    const model = String(modelId || '').trim();
    const file = String(filename || '').trim();
    if (!/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/.test(model)) {
        throw new Error('Identificador de modelo inválido. Use o formato organização/modelo.');
    }
    if (!/^[A-Za-z0-9._-]+\.gguf$/i.test(file) || path.basename(file) !== file) {
        throw new Error('Nome de arquivo GGUF inválido. Caminhos e subpastas não são permitidos.');
    }
    return { modelId: model, filename: file };
}

function getDownloadPath(filename) {
    const target = path.resolve(DOWNLOADS_DIR, filename);
    const root = path.resolve(DOWNLOADS_DIR) + path.sep;
    if (!target.startsWith(root)) throw new Error('Destino do modelo fora da pasta autorizada.');
    return target;
}

function downloadFile(url, filename, onProgress) {
    return new Promise((resolve, reject) => {
        const destPath = getDownloadPath(filename);
        const tempPath = destPath + '.downloading';
        
        const file = fs.createWriteStream(tempPath);
        let downloadedBytes = 0;
        let totalBytes = 0;
        
        https.get(url, { headers: { 'User-Agent': 'NEXA-AI/1.0' } }, (res) => {
            if (res.statusCode === 302 || res.statusCode === 301) {
                // Follow redirect
                https.get(res.headers.location, (res2) => {
                    totalBytes = parseInt(res2.headers['content-length'] || '0');
                    res2.pipe(file);
                    res2.on('data', (chunk) => {
                        downloadedBytes += chunk.length;
                        if (onProgress) onProgress({ downloaded: downloadedBytes, total: totalBytes, percent: totalBytes ? ((downloadedBytes/totalBytes)*100).toFixed(1) : 0 });
                    });
                    file.on('finish', () => {
                        file.close();
                        fs.renameSync(tempPath, destPath);
                        resolve(destPath);
                    });
                }).on('error', reject);
                return;
            }
            
            totalBytes = parseInt(res.headers['content-length'] || '0');
            res.pipe(file);
            res.on('data', (chunk) => {
                downloadedBytes += chunk.length;
                if (onProgress) onProgress({ downloaded: downloadedBytes, total: totalBytes, percent: totalBytes ? ((downloadedBytes/totalBytes)*100).toFixed(1) : 0 });
            });
            file.on('finish', () => {
                file.close();
                fs.renameSync(tempPath, destPath);
                resolve(destPath);
            });
        }).on('error', (err) => {
            fs.unlink(tempPath, () => {});
            reject(err);
        });
    });
}

function getDownloadUrl(modelId, filename) {
    const encodedModel = modelId.split('/').map(encodeURIComponent).join('/');
    return `https://huggingface.co/${encodedModel}/resolve/main/${encodeURIComponent(filename)}`;
}

function startDownload(modelId, filename) {
    ({ modelId, filename } = validateDownloadRequest(modelId, filename));
    const downloadId = `${modelId}/${filename}`;
    if (activeDownloads.has(downloadId)) return { error: 'Download ja em andamento' };
    
    const url = getDownloadUrl(modelId, filename);
    const promise = downloadFile(url, filename, (progress) => {
        const dl = activeDownloads.get(downloadId);
        if (dl) dl.progress = progress;
    }).then((destPath) => {
        const dl = activeDownloads.get(downloadId);
        if (dl) Object.assign(dl, { status: 'complete', path: destPath, progress: { ...dl.progress, percent: 100 } });
        return { success: true, path: destPath };
    }).catch((err) => {
        const dl = activeDownloads.get(downloadId);
        if (dl) Object.assign(dl, { status: 'error', error: err.message });
        return { success: false, error: err.message };
    });
    
    activeDownloads.set(downloadId, { id: downloadId, status: 'downloading', progress: { downloaded: 0, total: 0, percent: 0 }, promise, modelId, filename });
    return { downloadId, status: 'started' };
}

function getProgress(downloadId) {
    const dl = activeDownloads.get(downloadId);
    if (!dl) return null;
    return { id: dl.id, modelId: dl.modelId, filename: dl.filename, status: dl.status, path: dl.path, error: dl.error, ...dl.progress };
}

function getAllDownloads() {
    const results = [];
    activeDownloads.forEach((dl, id) => {
        results.push({ id, modelId: dl.modelId, filename: dl.filename, status: dl.status, path: dl.path, error: dl.error, ...dl.progress });
    });
    return results;
}

function cancelDownload(downloadId) {
    const dl = activeDownloads.get(downloadId);
    if (dl) {
        activeDownloads.delete(downloadId);
        // Tentar remover arquivo parcial
        const tempPath = getDownloadPath(dl.filename) + '.downloading';
        try { fs.unlinkSync(tempPath); } catch(e) {}
        return { success: true };
    }
    return { error: 'Download nao encontrado' };
}

function getAvailableModels() {
    if (!fs.existsSync(DOWNLOADS_DIR)) return [];
    return fs.readdirSync(DOWNLOADS_DIR)
        .filter(f => f.endsWith('.gguf'))
        .map(f => {
            const stats = fs.statSync(path.join(DOWNLOADS_DIR, f));
            return { filename: f, path: path.join(DOWNLOADS_DIR, f), sizeGB: (stats.size / (1024*1024*1024)).toFixed(2) };
        });
}

function getDiskSpace() {
    try { return fs.statfsSync(DOWNLOADS_DIR); } catch(e) { return null; }
}

module.exports = { startDownload, getProgress, getAllDownloads, cancelDownload, getAvailableModels, getDiskSpace, validateDownloadRequest, getDownloadPath, DOWNLOADS_DIR };
