const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { isUsableModel, assessCompatibility, getRunningModel, stableGpuLayers, parseParamsB, classifyModelKind, rateModelRun } = require('./modelManager');

test('keeps embedding and reranking GGUFs out of the chat model selector', () => {
    assert.equal(isUsableModel('C:/app/backend/models/embeddings/nomic-embed-text-v1.5.Q4_K_M.gguf'), false);
    assert.equal(isUsableModel('D:/models/bge-m3-Q4_K_M.gguf'), false);
    assert.equal(isUsableModel('D:/models/my-reranker.gguf'), false);
});

test('accepts ordinary local chat and coding GGUFs', () => {
    assert.equal(isUsableModel('D:/models/Qwen3.5-4B-Q4_K_M.gguf'), true);
    assert.equal(isUsableModel('D:/models/deepseek-coder-6.7b.Q4_K_M.gguf'), true);
});

test('marks a GGUF with a known unsupported architecture as incompatible', () => {
    const result = assessCompatibility('D:/models/DeepSeek-V4-Flash-DSpark-support.gguf');
    assert.equal(result.status, 'unsupported');
    assert.match(result.reason, /deepseek4-dspark/i);
});

test('uses CPU mode by default and only enables an explicit GPU layer count', () => {
    assert.equal(stableGpuLayers(undefined), '0');
    assert.equal(stableGpuLayers('10'), '10');
    assert.equal(stableGpuLayers('-1'), '0');
});

test('reads the model actually served by an OpenAI-compatible endpoint', async () => {
    const server = http.createServer((_req, res) => {
        res.setHeader('Content-Type', 'application/json');
        res.end(JSON.stringify({ data: [{ id: 'D:/models/active-model.gguf' }] }));
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
        const { port } = server.address();
        assert.equal(await getRunningModel(`http://127.0.0.1:${port}`), 'D:/models/active-model.gguf');
    } finally {
        await new Promise(resolve => server.close(resolve));
    }
});

test('parses size labels of dense and MoE models', () => {
    assert.deepEqual(parseParamsB('7.6B'), { totalB: 7.6, activeB: 7.6, moe: false });
    assert.deepEqual(parseParamsB('256x8.4B'), { totalB: 256, activeB: 8.4, moe: true });
    assert.deepEqual(parseParamsB('1.5B'), { totalB: 1.5, activeB: 1.5, moe: false });
    assert.equal(parseParamsB('nada'), null);
});

test('classifies auxiliary/non-chat GGUFs by file name', () => {
    const emptyMeta = { arch: null, paramsB: null, sizeLabel: null, context: null };
    assert.equal(classifyModelKind('D:/models/gguf-lora-llama3-midjourney.Q4_K_M.gguf', emptyMeta), 'non-llm');
    assert.equal(classifyModelKind('D:/models/qwen3-lora.gguf', emptyMeta), 'non-llm');
    assert.equal(classifyModelKind('D:/models/clip-vit.Q4_K_M.gguf', emptyMeta), 'non-llm');
    assert.equal(classifyModelKind('D:/models/yolov3-tiny.gguf', emptyMeta), 'non-llm');
    assert.equal(classifyModelKind('D:/models/whisper-1.gguf', emptyMeta), 'non-llm');
});

test('classifies chat GGUFs as LLM despite auxiliary-looking words', () => {
    const emptyMeta = { arch: null, paramsB: null, sizeLabel: null, context: null };
    assert.equal(classifyModelKind('D:/models/adi-qwen2.5-coder-7b-kimi-q4_k_m.gguf', emptyMeta), 'llm');
    assert.equal(classifyModelKind('D:/models/deepseek-coder-6.7b-instruct.Q4_K_M.gguf', emptyMeta), 'llm');
});

test('rates run feasibility: small models stay green, huge models are red', () => {
    // Usa um GGUF mínimo real temporário para exercitar o parser de header.
    const tmp = path.join(os.tmpdir(), 'nexa-test-model-run.gguf');
    try {
        fs.writeFileSync(tmp, Buffer.from('GGUF'));
        const small = rateModelRun(tmp, 1.0);
        assert.ok(['excellent', 'good'].includes(small.category), `esperado excelente/bom, veio ${small.category}`);
        assert.ok(small.fitsRam !== false);
        const big = rateModelRun(tmp, 80.0);
        assert.equal(big.category, 'extreme');
        assert.equal(big.color, 'red');
        assert.equal(big.fitsRam, false);
        assert.match(big.reason, /RAM/i);
        const aux = rateModelRun(tmp.replace('nexa-test-model-run', 'nexa-test-lora'), 1.0);
        assert.equal(aux.category, 'unsupported');
    } finally {
        try { fs.unlinkSync(tmp); } catch {}
    }
});
