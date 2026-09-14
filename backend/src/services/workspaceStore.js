const fs = require('fs');
const path = require('path');
const runtimePaths = require('../config/runtimePaths');

function storeFile() {
    return path.join(runtimePaths.dataDir(), 'workspaces.json');
}

function readAll() {
    try {
        const value = JSON.parse(fs.readFileSync(storeFile(), 'utf8'));
        return Array.isArray(value) ? value : [];
    } catch { return []; }
}

function writeAll(workspaces) {
    fs.mkdirSync(path.dirname(storeFile()), { recursive: true });
    fs.writeFileSync(storeFile(), JSON.stringify(workspaces, null, 2), 'utf8');
}

function listWorkspaces() {
    return readAll().sort((a, b) => String(b.updatedAt).localeCompare(String(a.updatedAt)));
}

function getWorkspace(id) {
    return readAll().find(item => item.id === String(id)) || null;
}

function findByPath(projectPath) {
    const normalized = String(projectPath || '').toLocaleLowerCase();
    return readAll().find(item => String(item.projectPath || '').toLocaleLowerCase() === normalized) || null;
}

function createWorkspace({ name, projectPath, instructions = '' } = {}) {
    const existing = projectPath ? findByPath(projectPath) : null;
    if (existing) return existing;
    const now = new Date().toISOString();
    const workspace = {
        id: `ws_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`,
        name: String(name || 'Projeto').slice(0, 120),
        projectPath: projectPath || null,
        instructions: String(instructions || '').slice(0, 4000),
        createdAt: now,
        updatedAt: now
    };
    const all = readAll();
    all.push(workspace);
    writeAll(all);
    return workspace;
}

function updateWorkspace(id, patch = {}) {
    const all = readAll();
    const index = all.findIndex(item => item.id === String(id));
    if (index < 0) return null;
    const current = all[index];
    const updated = {
        ...current,
        ...(typeof patch.name === 'string' ? { name: patch.name.slice(0, 120) } : {}),
        ...(typeof patch.instructions === 'string' ? { instructions: patch.instructions.slice(0, 4000) } : {}),
        updatedAt: new Date().toISOString()
    };
    all[index] = updated;
    writeAll(all);
    return updated;
}

function deleteWorkspace(id) {
    const all = readAll();
    const remaining = all.filter(item => item.id !== String(id));
    if (remaining.length === all.length) return false;
    writeAll(remaining);
    return true;
}

module.exports = { listWorkspaces, getWorkspace, findByPath, createWorkspace, updateWorkspace, deleteWorkspace };
