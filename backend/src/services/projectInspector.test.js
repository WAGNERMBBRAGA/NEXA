const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { inspectProject, summarizeInspection } = require('./projectInspector');

test('builds a structural inventory and reports only verified file findings', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-inspection-'));
    try {
        fs.mkdirSync(path.join(root, 'src'));
        fs.mkdirSync(path.join(root, 'tests'));
        fs.writeFileSync(path.join(root, 'package.json'), '{ invalid', 'utf8');
        fs.writeFileSync(path.join(root, 'src', 'app.js'), '<<<<<<< HEAD\nconst value = 1;\n=======\nconst value = 2;\n>>>>>>> branch', 'utf8');
        fs.writeFileSync(path.join(root, 'src', 'empty.ts'), '', 'utf8');
        fs.writeFileSync(path.join(root, 'tests', 'app.test.js'), 'test("ok", () => {});', 'utf8');

        const report = inspectProject(root);
        assert.equal(report.files, 4);
        assert.equal(report.contentInspectedFiles, 4);
        assert.deepEqual(report.topDirectories, ['src', 'tests']);
        assert.ok(report.findings.some(finding => finding.code === 'invalid_json' && finding.file === 'package.json'));
        assert.ok(report.findings.some(finding => finding.code === 'merge_conflict_marker' && finding.file === 'src/app.js'));
        assert.ok(report.findings.some(finding => finding.code === 'empty_source_file' && finding.file === 'src/empty.ts'));
        assert.match(summarizeInspection(report), /Achados estruturais comprovados/);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('does not present a clean project as having structural inconsistencies', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-inspection-'));
    try {
        fs.writeFileSync(path.join(root, 'package.json'), '{"scripts":{"test":"node --test"}}', 'utf8');
        const report = inspectProject(root);
        assert.equal(report.findings.length, 0);
        assert.match(summarizeInspection(report), /Nenhuma inconsistência estrutural objetiva/);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('identifies Python architecture and ignores installed dependency artifacts', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-inspection-'));
    try {
        fs.mkdirSync(path.join(root, '__pycache__'));
        fs.mkdirSync(path.join(root, 'joblib-1.5.2.dist-info'));
        fs.mkdirSync(path.join(root, 'trader'));
        fs.writeFileSync(path.join(root, '__pycache__', 'main.pyc'), 'cache', 'utf8');
        fs.writeFileSync(path.join(root, 'joblib-1.5.2.dist-info', 'METADATA'), 'dependency metadata', 'utf8');
        fs.writeFileSync(path.join(root, 'requirements.txt'), 'pandas==2.2.0\njoblib==1.5.2\n', 'utf8');
        fs.writeFileSync(path.join(root, 'main.py'), 'def main():\n    return True\n', 'utf8');
        fs.writeFileSync(path.join(root, 'trader', 'strategy.py'), 'class Strategy:\n    pass\n', 'utf8');
        const report = inspectProject(root);
        assert.deepEqual(report.topDirectories, ['trader']);
        assert.deepEqual(report.architecture.ecosystems, ['Python']);
        assert.ok(report.architecture.entryPoints.includes('main.py'));
        assert.ok(report.architecture.frameworks.includes('pandas'));
        assert.equal(report.findings.some(finding => /dist-info|__pycache__/.test(finding.file)), false);
        assert.ok(report.findings.some(finding => finding.code === 'missing_tests'));
        assert.ok(report.findings.some(finding => finding.code === 'missing_readme'));
        assert.match(summarizeInspection(report), /Arquitetura identificada por evidências/);
        assert.match(summarizeInspection(report), /Correções recomendadas a partir desses achados/);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('does not flag divergent dependency manifests when their scopes are documented', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-inspection-'));
    try {
        fs.mkdirSync(path.join(root, 'bot'));
        fs.mkdirSync(path.join(root, 'tests'));
        fs.writeFileSync(path.join(root, 'requirements.txt'), 'flask==3.0.0\n', 'utf8');
        fs.writeFileSync(path.join(root, 'bot', 'requirements.txt'), 'pandas==2.2.0\n', 'utf8');
        fs.writeFileSync(path.join(root, 'bot', 'main.py'), 'def main():\n    return True\n', 'utf8');
        fs.writeFileSync(path.join(root, 'tests', 'test_main.py'), 'def test_main():\n    assert True\n', 'utf8');
        fs.writeFileSync(path.join(root, 'DEPENDENCIES.md'), '# Dependências\n\n- `requirements.txt`\n- `bot/requirements.txt`\n', 'utf8');

        const report = inspectProject(root);
        assert.equal(report.findings.some(finding => finding.code === 'divergent_manifests'), false);
        assert.ok(report.notes.some(note => note.file === 'DEPENDENCIES.md'));
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});
