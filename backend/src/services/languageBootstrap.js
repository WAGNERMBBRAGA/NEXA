const fs = require('fs');
const path = require('path');

const text = lines => lines.join('\n') + '\n';

function normalizeLanguage(language) {
    const lang = String(language || '').toLowerCase();
    if (lang.includes('python') || lang === 'py' || lang.includes('django')) return 'python';
    if (lang.includes('javascript') || lang.includes('node') || lang === 'js') return 'javascript';
    if (lang.includes('typescript') || lang === 'ts') return 'typescript';
    if (lang.includes('c#') || lang.includes('csharp') || lang.includes('.net')) return 'csharp';
    if (lang.includes('java')) return 'java';
    if (lang.includes('php') || lang.includes('laravel')) return 'php';
    if (lang.includes('go')) return 'go';
    if (lang.includes('ruby') || lang.includes('rails')) return 'ruby';
    if (lang.includes('rust')) return 'rust';
    if (lang.includes('kotlin')) return 'kotlin';
    if (lang.includes('swift')) return 'swift';
    if (lang.includes('c++') || lang.includes('cpp')) return 'cpp';
    if (lang === 'c' || lang.includes(' linguagem c')) return 'c';
    return 'python';
}

function mainSource(language, title) {
    const name = title || 'sistema';
    switch (normalizeLanguage(language)) {
        case 'python':
            return text([
                '"""' + name + ' - ponto de entrada."""',
                '',
                'def main():',
                '    print("NEXA: ' + name + ' inicializado")',
                '',
                'if __name__ == "__main__":',
                '    main()'
            ]);
        case 'javascript':
            return text([
                "'use strict';",
                '',
                'function main() {',
                '    console.log("NEXA: ' + name + ' inicializado");',
                '}',
                '',
                'if (require.main === module) main();',
                '',
                'module.exports = { main };'
            ]);
        case 'typescript':
            return text([
                'function main(): void {',
                '    console.log("NEXA: ' + name + ' inicializado");',
                '}',
                '',
                'export { main };'
            ]);
        case 'csharp':
            return text([
                'using System;',
                '',
                'public static class Program',
                '{',
                '    public static void Main()',
                '    {',
                '        Console.WriteLine("NEXA: ' + name + ' inicializado");',
                '    }',
                '}'
            ]);
        case 'java':
            return text([
                'public class Main {',
                '    public static void main(String[] args) {',
                '        System.out.println("NEXA: ' + name + ' inicializado");',
                '    }',
                '}'
            ]);
        case 'php':
            return text([
                '<?php',
                'declare(strict_types=1);',
                '',
                'echo "NEXA: ' + name + ' inicializado", PHP_EOL;',
                ''
            ]);
        case 'go':
            return text([
                'package main',
                '',
                'import "fmt"',
                '',
                'func main() {',
                '    fmt.Println("NEXA: ' + name + ' inicializado")',
                '}'
            ]);
        case 'ruby':
            return text([
                'puts "NEXA: ' + name + ' inicializado"',
                ''
            ]);
        case 'rust':
            return text([
                'fn main() {',
                '    println!("NEXA: ' + name + ' inicializado");',
                '}'
            ]);
        case 'kotlin':
            return text([
                'fun main() {',
                '    println("NEXA: ' + name + ' inicializado")',
                '}'
            ]);
        case 'swift':
            return text([
                'print("NEXA: ' + name + ' inicializado")'
            ]);
        case 'cpp':
            return text([
                '#include <iostream>',
                '',
                'int main() {',
                '    std::cout << "NEXA: ' + name + ' inicializado" << std::endl;',
                '    return 0;',
                '}'
            ]);
        case 'c':
            return text([
                '#include <stdio.h>',
                '',
                'int main(void) {',
                '    printf("NEXA: ' + name + ' inicializado\\n");',
                '    return 0;',
                '}'
            ]);
        default:
            return text(['print("NEXA: ' + name + ' inicializado")']);
    }
}

function mainPath(language) {
    switch (normalizeLanguage(language)) {
        case 'python': return 'src/main.py';
        case 'javascript': return 'src/main.js';
        case 'typescript': return 'src/main.ts';
        case 'csharp': return 'src/Program.cs';
        case 'java': return 'src/main/java/Main.java';
        case 'php': return 'src/main.php';
        case 'go': return 'src/main.go';
        case 'ruby': return 'src/main.rb';
        case 'rust': return 'src/main.rs';
        case 'kotlin': return 'src/main/kotlin/Main.kt';
        case 'swift': return 'src/main.swift';
        case 'cpp': return 'src/main.cpp';
        case 'c': return 'src/main.c';
        default: return 'src/main.py';
    }
}

function projectTitle(objective) {
    const words = String(objective || '')
        .replace(/[.,;:!?]+/g, ' ')
        .replace(/\b(?:crie|criar|construa|construir|montar|sistema de um|sistema de|sistema|completo|completa|um|uma|o|a|os|as|de|da|do|das|dos|em|no|na|nos|nas|app|aplicativo|e|para)\b/gi, ' ')
        .trim()
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 3);
    if (!words.length) return 'Sistema';
    return words.map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
}

function languageFoundationActions(projectRoot, objective, language) {
    if (!projectRoot) return [];
    const mainFile = mainPath(language);
    if (fs.existsSync(path.join(projectRoot, 'README.md')) || fs.existsSync(path.join(projectRoot, ...mainFile.split('/')))) return [];
    const title = projectTitle(objective) || 'Sistema';
    const lang = normalizeLanguage(language);
    const readme = text([
        '# ' + title,
        '',
        'Sistema criado pelo NEXA em **' + language + '**.',
        '',
        '## Objetivo',
        '',
        (String(objective || '').trim() || 'Sistema solicitado nesta conversa.')
    ]);
    const config = '{ "nome": "' + title + '", "linguagem": "' + language + '", "estado": "fundacao" }' + '\n';
    return [
        { kind: 'create_file', path: 'README.md', content: readme },
        { kind: 'create_file', path: 'config.json', content: config },
        { kind: 'create_file', path: mainFile, content: mainSource(language, title) }
    ];
}

module.exports = { languageFoundationActions, normalizeLanguage, mainPath, languageBootstrapTitle: projectTitle };