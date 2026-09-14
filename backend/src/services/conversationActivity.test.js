const assert = require('node:assert/strict');
const test = require('node:test');
const { publish, subscribe } = require('./conversationActivity');

test('delivers activity only to the matching conversation', () => {
    const received = [];
    const stop = subscribe('conversation-a', event => received.push(event));
    publish('conversation-b', { label: 'Ignorar' });
    publish('conversation-a', { label: 'Projeto inspecionado' });
    stop();
    assert.deepEqual(received.map(event => event.label), ['Projeto inspecionado']);
});
