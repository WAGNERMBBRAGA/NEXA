const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { auditProject } = require('./projectAudit');

test('checks Python syntax without creating __pycache__ or scanning dist-info', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-audit-'));
    try {
        fs.mkdirSync(path.join(root, 'joblib-1.5.2.dist-info'));
        fs.writeFileSync(path.join(root, 'requirements.txt'), 'joblib==1.5.2\n', 'utf8');
        fs.writeFileSync(path.join(root, 'main.py'), 'def main():\n    return True\n', 'utf8');
        fs.writeFileSync(path.join(root, 'joblib-1.5.2.dist-info', 'METADATA'), 'ignored', 'utf8');
        const report = auditProject(root);
        const python = report.checks.find(check => check.name === 'python_syntax');
        assert.equal(python.ok, true);
        assert.equal(fs.existsSync(path.join(root, '__pycache__')), false);
        assert.equal(report.unreadable.length, 0);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
