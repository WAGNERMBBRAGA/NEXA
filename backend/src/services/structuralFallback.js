const fs = require('fs');
const path = require('path');

function relativeExists(root, relative) {
    return fs.existsSync(path.join(root, ...relative.split('/')));
}

function pythonSyntaxTest(entryFiles) {
    const entries = entryFiles.filter(file => file.toLowerCase().endsWith('.py'));
    const list = entries.map(file => `    ${JSON.stringify(file)},`).join('\n');
    return `"""Cobertura inicial: garante que os pontos de entrada Python compilam."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENTRY_POINTS = [
${list}
]


class EntryPointSyntaxTests(unittest.TestCase):
    def test_entry_points_compile(self):
        for relative_path in ENTRY_POINTS:
            source = (ROOT / relative_path).read_text(encoding="utf-8")
            compile(source, relative_path, "exec")


if __name__ == "__main__":
    unittest.main()
`;
}

function dependencyGuide(manifests) {
    const items = manifests.map(file => `- \`${file}\``).join('\n');
    return `# Dependências do projeto

Este projeto possui manifestos de dependências em escopos diferentes. Eles não
devem ser mesclados automaticamente, pois cada pasta pode representar uma
aplicação ou módulo independente.

## Manifestos identificados

${items}

Instale as dependências a partir da pasta que contém o componente que será
executado. Se os componentes passarem a compartilhar um único ambiente, revise
os manifestos e consolide apenas depois de validar os pontos de entrada.
`;
}

function structuralFallbackActions(root, inspection = {}) {
    const findings = inspection.findings || [];
    const codes = new Set(findings.map(finding => finding.code));
    const actions = [];
    // A ausência de testes é um achado de auditoria, não autorização para criar
    // arquivos genéricos. Só criamos cobertura quando ela for pedida para uma
    // funcionalidade concreta desta conversa.
    if ((codes.has('divergent_manifests') || codes.has('duplicate_manifests')) && !relativeExists(root, 'DEPENDENCIES.md')) {
        actions.push({ kind: 'create_file', path: 'DEPENDENCIES.md', content: dependencyGuide(inspection.manifests || []) });
    }
    return actions;
}

module.exports = { structuralFallbackActions };
