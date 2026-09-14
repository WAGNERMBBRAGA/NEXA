const assert = require('node:assert/strict');
const http = require('http');
const test = require('node:test');
const { requestEmbeddings, cosine } = require('./semanticIndex');

test('uses real OpenAI-compatible embedding vectors', async () => {
    const server = http.createServer((request, response) => {
        assert.equal(request.url, '/v1/embeddings');
        let body = '';
        request.on('data', chunk => { body += chunk; });
        request.on('end', () => {
            const payload = JSON.parse(body);
            response.setHeader('Content-Type', 'application/json');
            response.end(JSON.stringify({ data: payload.input.map((_, index) => ({ index, embedding: index ? [0, 1] : [1, 0] })) }));
        });
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
        const address = server.address();
        const vectors = await requestEmbeddings({ baseUrl: `http://127.0.0.1:${address.port}`, model: 'real-test', apiKey: '' }, ['a', 'b']);
        assert.deepEqual(vectors, [[1, 0], [0, 1]]);
        assert.equal(cosine(vectors[0], vectors[0]), 1);
        assert.equal(cosine(vectors[0], vectors[1]), 0);
    } finally {
        await new Promise(resolve => server.close(resolve));
    }
});

test('does not duplicate v1 in an embedding provider base URL', async () => {
    const server = http.createServer((request, response) => {
        assert.equal(request.url, '/v1/embeddings');
        response.setHeader('Content-Type', 'application/json');
        response.end(JSON.stringify({ data: [{ index: 0, embedding: [1, 2] }] }));
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
        const address = server.address();
        const vectors = await requestEmbeddings({ baseUrl: `http://127.0.0.1:${address.port}/v1`, model: 'real-test', apiKey: '' }, ['a']);
        assert.deepEqual(vectors, [[1, 2]]);
    } finally {
        await new Promise(resolve => server.close(resolve));
    }
});

test('rejects a successful response containing unusable vectors', async () => {
    const server = http.createServer((_request, response) => {
        response.setHeader('Content-Type', 'application/json');
        response.end(JSON.stringify({ data: [{ index: 0, embedding: [null, null] }] }));
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
        const address = server.address();
        await assert.rejects(
            requestEmbeddings({ baseUrl: `http://127.0.0.1:${address.port}`, model: 'real-test', apiKey: '' }, ['a']),
            /vetores válidos/
        );
    } finally {
        await new Promise(resolve => server.close(resolve));
    }
});
