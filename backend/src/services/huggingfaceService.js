const https = require('https');

const HF_API = 'https://huggingface.co/api/models';

function fetchJSON(url) {
    return new Promise((resolve, reject) => {
        https.get(url, { headers: { 'User-Agent': 'NEXA-AI/1.0' } }, (res) => {
            let data = '';
            res.on('data', chunk => data += chunk);
            res.on('end', () => {
                try { resolve(JSON.parse(data)); }
                catch(e) { reject(new Error('Parse error')); }
            });
        }).on('error', reject);
    });
}

﻿async function getTreeSizes(modelId) {
    try {
        const url = HF_API + '/' + modelId + '/tree/main';
        const files = await fetchJSON(url);
        const sizes = {};
        files.filter(f => f.path && f.path.endsWith('.gguf')).forEach(f => {
            sizes[f.path] = f.size || 0;
            const fname = f.path.split('/').pop();
            if (!sizes[fname]) sizes[fname] = f.size || 0;
        });
        const dirs = files.filter(f => f.type === 'directory');
        for (const dir of dirs) {
            try {
                const subUrl = HF_API + '/' + modelId + '/tree/main/' + dir.path;
                const subFiles = await fetchJSON(subUrl);
                subFiles.filter(f => f.path && f.path.endsWith('.gguf')).forEach(f => {
                    sizes[f.path] = f.size || 0;
                    const fname = f.path.split('/').pop();
                    if (!sizes[fname]) sizes[fname] = f.size || 0;
                });
            } catch(e) {}
        }
        return sizes;
    } catch(e) { return {}; }
}

async function listModels({ query = '', sort = 'downloads', limit = 24, task = '', vision = false } = {}) {
    const searchQuery = query ? `${query} gguf` : 'gguf';
    let url = `${HF_API}?search=${encodeURIComponent(searchQuery)}&sort=${sort}&direction=-1&limit=${limit}&full=true`;
    if (task && task !== 'text-generation') url += `&pipeline_tag=${task}`;
    
    const models = await fetchJSON(url);
    const results = [];
    
    for (const m of models) {
        const siblings = m.siblings || [];
        const ggufFiles = siblings.filter(f => f.rfilename && f.rfilename.endsWith('.gguf'));
        if (ggufFiles.length === 0) continue;
        
        // Buscar tamanhos via Tree API (apenas primeiros 3 modelos para nao exceder rate limit)
        const sizes = await getTreeSizes(m.id);
        
        const hasVision = (m.pipeline_tag === 'image-to-text' || m.pipeline_tag === 'visual-question-answering')
            || (m.tags || []).some(t => /vision|vl|llava|moondream|multimodal/i.test(t));
        
        const variants = ggufFiles.map(f => {
            const fname = f.rfilename.split('/').pop();
            const size = sizes[fname] || sizes[f.rfilename] || 0;
            return {
                filename: f.rfilename,
                size,
                sizeGB: size ? (size / (1024*1024*1024)).toFixed(2) : '?',
                quantization: (fname.match(/\.(Q[0-9K]+[_A-Z0-9]*)\./i) || [])[1] || 'unknown'
            };
        });
        
        const totalBytes = variants.reduce((s, v) => s + (v.size || 0), 0);
        const quantizations = [...new Set(variants.map(v => v.quantization).filter(q => q !== 'unknown'))];
        
        results.push({
            id: m.id,
            author: m.id.split('/')[0],
            name: m.id.split('/')[1],
            downloads: m.downloads || 0,
            likes: m.likes || 0,
            tags: (m.tags || []).slice(0, 8),
            pipeline: m.pipeline_tag || 'text-generation',
            hasVision,
            lastModified: m.lastModified,
            variants,
            totalSizeGB: totalBytes ? (totalBytes / (1024*1024*1024)).toFixed(2) : '?',
            quantizations
        });
    }
    
    return results;
}

module.exports = { listModels };