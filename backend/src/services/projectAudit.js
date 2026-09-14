const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const IGNORED = new Set(['node_modules', 'vendor', '.git', 'dist', 'build', 'storage', '__pycache__', 'site-packages', '.venv', 'venv', 'env']);

function auditProject(root) {
    const report = { files: 0, directories: 0, unreadable: [], manifests: [], config: [], checks: [] };
    const run = (name, command, args, cwd) => {
        const result = spawnSync(command, args, {
            cwd,
            // npm.cmd cannot be spawned directly with shell disabled on Windows.
            // Keep a shell only for npm, never for model-provided commands.
            shell: process.platform === 'win32' && ['npm', 'mvn', 'gradlew.bat'].includes(command),
            windowsHide: true,
            timeout: 120000,
            encoding: 'utf8',
        });
        report.checks.push({ name, ok: !result.error && result.status === 0, exitCode: result.status, output: String(result.stderr || result.stdout || result.error?.message || '').slice(0, 3000) });
    };
    function walk(dir) {
        let entries;
        try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch (error) { report.unreadable.push(path.relative(root, dir) || '.'); return; }
        for (const entry of entries) {
            const full = path.join(dir, entry.name);
            const relative = path.relative(root, full);
            if (entry.isDirectory()) {
                if (IGNORED.has(entry.name) || /\.(?:dist|egg)-info$/i.test(entry.name)) continue;
                report.directories++;
                walk(full);
            } else if (entry.isFile()) {
                report.files++;
                if (['package.json', 'composer.json', 'docker-compose.yml', 'Cargo.toml', 'go.mod', 'pom.xml', 'build.gradle', 'build.gradle.kts'].includes(entry.name) || /\.(sln|csproj)$/i.test(entry.name)) report.manifests.push(relative);
                if (entry.name === '.env' || entry.name === '.env.example') report.config.push(relative);
                try { fs.accessSync(full, fs.constants.R_OK); } catch { report.unreadable.push(relative); }
            }
        }
    }
    walk(root);
    const runNames = new Set();
    const runOnce = (name, command, args, cwd) => {
        if (runNames.has(name)) return;
        runNames.add(name);
        run(name, command, args, cwd);
    };
    const laravelEnv = path.join(root, 'backend', '.env');
    const compose = path.join(root, 'docker-compose.yml');
    if (fs.existsSync(laravelEnv)) {
        const env = fs.readFileSync(laravelEnv, 'utf8');
        const value = key => (env.match(new RegExp(`^${key}=(.*)$`, 'm')) || [])[1] || '';
        report.laravel = { database: value('DB_CONNECTION'), cache: value('CACHE_DRIVER'), queue: value('QUEUE_CONNECTION'), broadcast: value('BROADCAST_DRIVER') };
    }
    if (fs.existsSync(compose)) {
        const dockerText = fs.readFileSync(compose, 'utf8');
        report.docker = { declared: true, mysql: /\bmysql:/i.test(dockerText), redis: /\bredis:/i.test(dockerText) };
    }
    const developmentDoc = path.join(root, 'DEVELOPMENT.md');
    if (fs.existsSync(developmentDoc)) {
        const documentation = fs.readFileSync(developmentDoc, 'utf8');
        report.developmentModesDocumented = /sqlite/i.test(documentation) && /docker/i.test(documentation) && /mysql/i.test(documentation);
    } else {
        report.developmentModesDocumented = false;
    }
    report.phpunitAvailable = fs.existsSync(path.join(root, 'backend', 'vendor', 'phpunit', 'phpunit', 'phpunit'));
    const frontend = path.join(root, 'frontend');
    const backend = path.join(root, 'backend');
    if (fs.existsSync(path.join(frontend, 'package.json'))) runOnce('build_frontend', 'npm', ['run', 'build'], frontend);
    if (fs.existsSync(path.join(backend, 'artisan'))) {
        runOnce('laravel_about', 'php', ['artisan', 'about', '--no-interaction'], backend);
        runOnce('laravel_tests', 'php', ['artisan', 'test'], backend);
    }
    const rootPackage = path.join(root, 'package.json');
    if (fs.existsSync(rootPackage)) {
        try {
            const packageJson = JSON.parse(fs.readFileSync(rootPackage, 'utf8'));
            const scripts = packageJson.scripts || {};
            if (scripts.build) runOnce('node_build', 'npm', ['run', 'build'], root);
            if (scripts.test && !/no test specified/i.test(scripts.test)) runOnce('node_tests', 'npm', ['test'], root);
        } catch (error) {
            report.checks.push({ name: 'package_json', ok: false, exitCode: null, output: `package.json inválido: ${error.message}` });
        }
    }
    if (fs.existsSync(path.join(root, 'pyproject.toml')) || fs.existsSync(path.join(root, 'requirements.txt'))) {
        const syntaxCheck = "import ast,pathlib,sys\nskip={'__pycache__','site-packages','.venv','venv','env'}\nerrors=[]\nfor p in pathlib.Path('.').rglob('*.py'):\n    if any(x in skip or x.endswith(('.dist-info','.egg-info')) for x in p.parts): continue\n    try: ast.parse(p.read_text(encoding='utf-8-sig'), filename=str(p))\n    except Exception as e: errors.append(f'{p}: {e}')\nprint('\\n'.join(errors))\nsys.exit(1 if errors else 0)";
        runOnce('python_syntax', 'python', ['-c', syntaxCheck], root);
    }
    if (fs.existsSync(path.join(root, 'Cargo.toml'))) {
        runOnce('rust_check', 'cargo', ['check', '--workspace'], root);
    }
    if (fs.existsSync(path.join(root, 'go.mod'))) {
        runOnce('go_tests', 'go', ['test', './...'], root);
    }
    const rootEntries = fs.readdirSync(root, { withFileTypes: true });
    const dotnetTarget = rootEntries.find(entry => entry.isFile() && /\.(sln|csproj)$/i.test(entry.name));
    if (dotnetTarget) {
        runOnce('dotnet_tests', 'dotnet', ['test', dotnetTarget.name, '--nologo'], root);
    }
    if (fs.existsSync(path.join(root, 'pom.xml'))) {
        runOnce('java_maven_tests', 'mvn', ['test', '-q'], root);
    } else if (fs.existsSync(path.join(root, 'gradlew.bat'))) {
        runOnce('java_gradle_tests', 'gradlew.bat', ['test', '--no-daemon'], root);
    }
    // Docker só é uma dependência de validação quando o projeto declara uma
    // stack Docker. Não marque projetos comuns como críticos por isso.
    const localModeDocumented = report.laravel && report.laravel.database === 'sqlite' && report.developmentModesDocumented;
    if (report.docker && report.docker.declared && !localModeDocumented) run('docker_disponivel', 'docker', ['--version'], root);
    if (report.laravel && report.docker && report.laravel.database === 'sqlite' && report.docker.mysql && !report.developmentModesDocumented) {
        report.inconsistency = 'O Laravel ativo usa SQLite, mas Docker declara MySQL/Redis. É necessário escolher e documentar um único modo de desenvolvimento.';
    }
    return report;
}

function summarizeAudit(report) {
    const issues = [];
    if (Array.isArray(report.unreadable) && report.unreadable.length) {
        issues.push(`Arquivos ou pastas sem leitura: ${report.unreadable.slice(0, 5).join(', ')}.`);
    }
    if (report.inconsistency) issues.push(report.inconsistency);
    for (const check of report.checks || []) {
        if (check.ok) continue;
        const output = String(check.output || '').replace(/\s+/g, ' ').trim();
        if (check.name === 'docker_disponivel') {
            issues.push(`Docker é declarado pelo projeto, mas não está disponível neste computador (${output || 'não foi possível iniciá-lo'}).`);
        } else {
            const label = {
                build_frontend: 'A compilação do frontend falhou',
                laravel_tests: 'Os testes do Laravel falharam',
                laravel_about: 'A verificação do Laravel falhou',
                node_build: 'A compilação Node falhou',
                node_tests: 'Os testes Node falharam',
                python_syntax: 'A validação de sintaxe Python falhou',
                rust_check: 'A validação Rust falhou',
                go_tests: 'Os testes Go falharam',
                dotnet_tests: 'Os testes .NET falharam',
                java_maven_tests: 'Os testes Maven falharam',
                java_gradle_tests: 'Os testes Gradle falharam',
                package_json: 'O manifesto Node é inválido'
            }[check.name] || `A validação ${check.name} falhou`;
            issues.push(`${label}${output ? `: ${output.slice(0, 300)}` : '.'}`);
        }
    }
    if (!issues.length) return 'Não encontrei erros críticos nas validações executadas.';
    return `Encontrei ${issues.length} problema${issues.length === 1 ? '' : 's'} verificado${issues.length === 1 ? '' : 's'}:\n\n${issues.map((issue, index) => `${index + 1}. ${issue}`).join('\n')}`;
}

module.exports = { auditProject, summarizeAudit };
