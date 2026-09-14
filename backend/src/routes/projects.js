const express = require('express');
const router = express.Router();
const projectService = require('../services/projectService');

router.post('/load', (req, res) => {
    try {
        const { path: folderPath } = req.body;
        if (!folderPath) return res.status(400).json({ success: false, error: 'Folder path required' });

        const fs = require('fs');
        if (!fs.existsSync(folderPath)) return res.status(400).json({ success: false, error: 'Folder not found: ' + folderPath });
        const stat = fs.statSync(folderPath);
        if (!stat.isDirectory()) return res.status(400).json({ success: false, error: 'Not a directory: ' + folderPath });

        const tree = projectService.scanFolder(folderPath);
        const info = projectService.getProjectStats(folderPath);
        const name = require('path').basename(folderPath);

        res.json({ success: true, data: { name, path: folderPath, tree, info } });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.get('/file', (req, res) => {
    try {
        const filePath = req.query.path;
        if (!filePath) return res.status(400).json({ success: false, error: 'File path required' });
        const result = projectService.readFileContent(filePath);
        res.json({ success: true, data: result });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

// Gravações só são aceitas pelo protocolo NEXA_ACTIONS da conversa, que
// confina cada caminho à raiz do projeto e registra o resultado da operação.
router.post(['/write', '/create'], (_req, res) => {
    res.status(410).json({
        success: false,
        error: 'Gravação direta removida. Solicite a alteração pela conversa do NEXA.'
    });
});

router.get('/info', (req, res) => {
    try {
        const folderPath = req.query.path;
        if (!folderPath) return res.status(400).json({ success: false, error: 'Path required' });
        const info = projectService.getProjectStats(folderPath);
        res.json({ success: true, data: info });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.get('/files', (req, res) => {
    try {
        const folderPath = req.query.path;
        if (!folderPath) return res.status(400).json({ success: false, error: 'Path required' });
        const files = projectService.listAllFiles(folderPath);
        res.json({ success: true, data: files });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.post('/read-all', (req, res) => {
    try {
        const { path: folderPath } = req.body;
        if (!folderPath) return res.status(400).json({ success: false, error: 'Path required' });
        const files = projectService.readAllProjectFiles(folderPath, 30000);
        res.json({ success: true, data: files });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.post('/read-batch', (req, res) => {
    try {
        const { path: folderPath, patterns } = req.body;
        if (!folderPath) return res.status(400).json({ success: false, error: 'Path required' });
        const files = projectService.readMultipleFiles(folderPath, patterns);
        res.json({ success: true, data: files });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.get('/browse', (req, res) => {
    try {
        const folderPath = req.query.path || '';
        const fs = require('fs');
        const path = require('path');
        
        if (!folderPath) {
            // Listar drives do Windows
            const { execSync } = require('child_process');
            try {
                const output = execSync('wmic logicaldisk get name', { encoding: 'utf-8' });
                const drives = output.split('\n').map(l => l.trim()).filter(l => /^[A-Z]:$/.test(l));
                return res.json({ success: true, data: { path: '', folders: drives.map(d => ({ name: d, path: d + '\\' })) } });
            } catch {
                return res.json({ success: true, data: { path: '', folders: [{ name: 'C:', path: 'C:\\' }, { name: 'D:', path: 'D:\\' }] } });
            }
        }

        if (!fs.existsSync(folderPath) || !fs.statSync(folderPath).isDirectory()) {
            return res.json({ success: true, data: { path: folderPath, folders: [] } });
        }

        const items = fs.readdirSync(folderPath, { withFileTypes: true });
        const folders = items
            .filter(i => i.isDirectory() && !i.name.startsWith('.') && !['node_modules', '__pycache__', '.git', 'dist', 'build'].includes(i.name))
            .map(i => ({ name: i.name, path: path.join(folderPath, i.name) }))
            .sort((a, b) => a.name.localeCompare(b.name));

        res.json({ success: true, data: { path: folderPath, folders } });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

module.exports = router;
