const fs = require('fs');
const path = require('path');
const runtimePaths = require('../config/runtimePaths');

const DATA_DIR = runtimePaths.dataDir();
const SESSION_FILE = path.join(DATA_DIR, 'active-session.json');

function read() {
    try { return JSON.parse(fs.readFileSync(SESSION_FILE, 'utf8')); }
    catch { return { conversationId: null, updatedAt: null }; }
}

function setActiveConversation(conversationId) {
    fs.mkdirSync(DATA_DIR, { recursive: true });
    const state = { conversationId: conversationId || null, updatedAt: new Date().toISOString() };
    const temporary = SESSION_FILE + '.tmp';
    fs.writeFileSync(temporary, JSON.stringify(state, null, 2), 'utf8');
    fs.renameSync(temporary, SESSION_FILE);
    return state;
}

module.exports = { read, setActiveConversation };
