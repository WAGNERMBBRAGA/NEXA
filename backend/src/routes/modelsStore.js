const express = require('express');
const router = express.Router();
const huggingfaceService = require('../services/huggingfaceService');
const downloadManager = require('../services/downloadManager');

router.get('/', async (req, res) => {
    try {
        const { q = '', sort = 'downloads', limit = 24, task, vision } = req.query;
        const models = await huggingfaceService.listModels({
            query: q, sort, limit: parseInt(limit), task, vision: vision === 'true'
        });
        res.json({ success: true, data: models });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.get('/local', (req, res) => {
    try {
        const models = downloadManager.getAvailableModels();
        const diskSpace = downloadManager.getDiskSpace();
        res.json({ success: true, data: { models, diskSpace, downloadsDir: downloadManager.DOWNLOADS_DIR } });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.post('/download', (req, res) => {
    try {
        const { modelId, filename } = req.body;
        if (!modelId || !filename) return res.status(400).json({ success: false, error: 'modelId e filename obrigatorios' });
        const result = downloadManager.startDownload(modelId, filename);
        res.json({ success: true, data: result });
    } catch (error) {
        res.status(400).json({ success: false, error: error.message });
    }
});

router.get('/download/progress', (req, res) => {
    try {
        const downloads = downloadManager.getAllDownloads();
        res.json({ success: true, data: downloads });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

router.delete('/download/:id', (req, res) => {
    try {
        const result = downloadManager.cancelDownload(req.params.id);
        res.json({ success: true, data: result });
    } catch (error) {
        res.status(500).json({ success: false, error: error.message });
    }
});

module.exports = router;
