/**
 * Chat API routes handler
 */

const aiService = require('../services/aiService');

// ============================================================================
// ROUTES HANDLER (to be mounted by server.js)
// ============================================================================

module.exports.getRoutes = function(app) {

    // Get AI status health check
    app.get('/api/chat/status', async (req, res) => {
        try {
            const providers = await aiService.getProviderStatus();

            res.json({
                success: true,
                data: {
                    providers: providers.map(p => ({
                        name: p.name,
                        status: p.status,
                        type: p.type
                    })),
                    availableModels: []
                }
            });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    // Get all available models (for selector dropdown)
    app.get('/api/chat/models', async (req, res) => {
        try {
            const provider = req.query.provider || 'local';
            const models = await aiService.getAvailableModels(provider);

            res.json({
                success: true,
                data: models
            });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    // Ollama endpoints
    app.get('/api/ollama/models', async (req, res) => {
        try {
            const ollamaService = require('../services/ollamaService');
            const models = await ollamaService.listModels(req.query.baseUrl);
            res.json({ success: true, data: models });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    app.post('/api/ollama/pull', async (req, res) => {
        try {
            const ollamaService = require('../services/ollamaService');
            const { model, baseUrl } = req.body || {};
            if (!model) return res.status(400).json({ success: false, error: 'Model name required' });
            const result = await ollamaService.pullModel(model, baseUrl);
            res.json({ success: true, data: result });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    app.delete('/api/ollama/model/:name', async (req, res) => {
        try {
            const ollamaService = require('../services/ollamaService');
            const result = await ollamaService.deleteModel(req.params.name, req.query.baseUrl);
            res.json({ success: true, data: result });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    // Kimi K3 endpoints
    app.get('/api/kimik3/status', async (req, res) => {
        try {
            const kimiK3Service = require('../services/kimiK3Service');
            const available = await kimiK3Service.isAvailable();
            const presets = kimiK3Service.listPresets();
            res.json({ success: true, data: { available, presets } });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    app.get('/api/kimik3/presets', async (req, res) => {
        try {
            const kimiK3Service = require('../services/kimiK3Service');
            res.json({ success: true, data: kimiK3Service.listPresets() });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    app.post('/api/kimik3/run', async (req, res) => {
        try {
            const kimiK3Service = require('../services/kimiK3Service');
            const { modelDir, trunkDir, prompt, preset, genTokens } = req.body || {};
            if (!modelDir) return res.status(400).json({ success: false, error: 'modelDir required' });
            const result = await kimiK3Service.runInference({
                modelDir, trunkDir, prompt, preset, genTokens
            });
            res.json({ success: true, data: result });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    app.post('/api/kimik3/check', async (req, res) => {
        try {
            const kimiK3Service = require('../services/kimiK3Service');
            const result = await kimiK3Service.checkMachine(req.body.modelDir);
            res.json({ success: true, data: result });
        } catch (error) {
            res.status(500).json({ success: false, error: error.message });
        }
    });

    // Main chat endpoint
    app.post('/api/chat', async (req, res) => {
        try {
            const requestBody = req.body || {};

            if (typeof requestBody.providerIndex !== 'number') {
                requestBody.providerIndex = 0;
            }

            const response = await aiService.chat(requestBody);

            if (!response || response.success === false) {
                return res.status(502).json({
                    success: false,
                    error: (response && response.error) || 'Provedor de IA indisponível'
                });
            }

            res.json({
                success: true,
                data: response.data
            });
        } catch (error) {
            res.status(500).json({
                success: false,
                error: error.message
            });
        }
    });

    // CORS preflight
    app.options('/{*path}', (req, res) => {
        res.status(204).end();
    });
};
