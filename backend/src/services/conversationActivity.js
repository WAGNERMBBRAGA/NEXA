const { EventEmitter } = require('node:events');

const events = new EventEmitter();
events.setMaxListeners(100);

function publish(conversationId, activity) {
    const event = {
        ...(activity || {}),
        type: (activity && activity.type) || 'status',
        label: String((activity && activity.label) || 'NEXA está trabalhando…').slice(0, 240),
        text: String((activity && activity.text) || '').slice(0, 12000),
        at: new Date().toISOString()
    };
    console.log('[CONV-ACTIVITY] publish type:', event.type, 'path:', event.path || '-', 'label:', (event.label || '').slice(0, 40));
    events.emit(String(conversationId), event);
}

function subscribe(conversationId, listener) {
    const channel = String(conversationId);
    events.on(channel, listener);
    return () => events.off(channel, listener);
}

module.exports = { publish, subscribe };
