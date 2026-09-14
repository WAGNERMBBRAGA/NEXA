const fs = require('fs');
const path = require('path');

const BACKEND_ROOT = path.join(__dirname, '..', '..');

function userDataRoot() {
    const configured = String(process.env.NEXA_USER_DATA || '').trim();
    return configured ? path.resolve(configured) : BACKEND_ROOT;
}

function ensureUserDataRoot() {
    const root = userDataRoot();
    fs.mkdirSync(root, { recursive: true });
    return root;
}

function configFile() {
    return path.join(userDataRoot(), 'config.json');
}

function dataDir() {
    return path.join(userDataRoot(), 'data');
}

module.exports = { BACKEND_ROOT, userDataRoot, ensureUserDataRoot, configFile, dataDir };
