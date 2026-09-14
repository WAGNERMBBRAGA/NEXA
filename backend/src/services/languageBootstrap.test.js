const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const test = require('node:test');
const { languageFoundationActions, normalizeLanguage, mainPath, languageBootstrapTitle } = require('./languageBootstrap');

test('normalizeLanguage maps vernacular names to canonical ids', () => {
    assert.equal(normalizeLanguage('Python'), 'python');
    assert.equal(normalizeLanguage('JavaScript / Node.js'), 'javascript');
    assert.equal(normalizeLanguage('TypeScript'), 'typescript');
    assert.equal(normalizeLanguage('C# (.NET)'), 'csharp');
    assert.equal(normalizeLanguage('Java'), 'java');
    assert.equal(normalizeLanguage('PHP'), 'php');
    assert.equal(normalizeLanguage('Go'), 'go');
    assert.equal(normalizeLanguage('Ruby'), 'ruby');
    assert.equal(normalizeLanguage('Rust'), 'rust');
    assert.equal(normalizeLanguage('C++'), 'cpp');
    assert.equal(normalizeLanguage('C'), 'c');
    assert.equal(normalizeLanguage('qualquer coisa'), 'python');
});

test('mainPath maps each language to a folder-based entry point', () => {
    assert.equal(mainPath('Python'), 'src/main.py');
    assert.equal(mainPath('C#'), 'src/Program.cs');
    assert.equal(mainPath('JavaScript'), 'src/main.js');
    assert.equal(mainPath('Java'), 'src/main/java/Main.java');
    assert.equal(mainPath('Rust'), 'src/main.rs');
});

test('languageBootstrapTitle extracts a short project name from the objective', () => {
    assert.equal(languageBootstrapTitle('Crie um sistema de comanda para restaurante'), 'Comanda Restaurante');
    assert.equal(languageBootstrapTitle('Montar app de academias'), 'Academias');
    assert.equal(languageBootstrapTitle(''), 'Sistema');
});

test('languageFoundationActions creates readme, config and language main on empty root', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-lang-'));
    try {
        const actions = languageFoundationActions(root, 'Criar um sistema de comanda para restaurante', 'Python');
        assert.equal(actions.length, 3);
        assert.deepEqual(actions.map(a => a.path), ['README.md', 'config.json', 'src/main.py']);
        assert.match(actions[0].content, /Comanda Restaurante/);
        assert.match(actions[0].content, /Python/);
        assert.match(actions[2].content, /print\("NEXA: /);
        for (const action of actions) {
            const target = path.join(root, ...action.path.split('/'));
            fs.mkdirSync(path.dirname(target), { recursive: true });
            fs.writeFileSync(target, action.content);
        }
        assert.match(fs.readFileSync(path.join(root, 'src/main.py'), 'utf8'), /def main\(\)/);
        assert.deepEqual(languageFoundationActions(root, 'Outra solicitação', 'Python'), []);
    } finally {
        fs.rmSync(root, { recursive: true, force: true });
    }
});

test('languageFoundationActions supports multiple languages with correct extensions', () => {
    const roots = [];
    try {
        for (const language of ['Python', 'JavaScript', 'TypeScript', 'C#', 'Java', 'PHP', 'Go', 'Ruby', 'Rust', 'Kotlin', 'Swift', 'C++', 'C']) {
            const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexa-lang-'));
            roots.push(root);
            const actions = languageFoundationActions(root, 'Criar sistema X', language);
            assert.equal(actions.length, 3, language);
            const mainFile = mainPath(language);
            assert.equal(actions[2].path, mainFile, language);
            assert.equal(actions[2].content.length > 0, true, language);
        }
    } finally {
        for (const root of roots) fs.rmSync(root, { recursive: true, force: true });
    }
});