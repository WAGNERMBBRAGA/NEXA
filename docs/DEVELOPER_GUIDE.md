# Guia de desenvolvimento

[Índice](INDEX.md) · [Contribuir](../CONTRIBUTING.md)

Comandos conferidos nos manifestos em 14/09/2026. Esta revisão não valida a instalação em máquina limpa.

## Preparação

Use Git, npm e Node.js compatível com o Vite instalado: `^20.19.0 || >=22.12.0`. O fluxo desktop tem foco em Windows. O frontend usa JavaScript, HTML, CSS e Vite; Docker, React, Prisma e PostgreSQL não são requisitos.

```powershell
git clone https://github.com/WAGNERMBBRAGA/NEXA.git
cd NEXA
npm ci
npm --prefix backend ci
npm --prefix frontend ci
```

## Motor e inicialização

O clone não inclui binários ou modelos. Disponibilize `llama-server.exe` e as bibliotecas do mesmo pacote em `llama.cpp/bin/`. A distribuição e versão do motor ainda precisam ser formalizadas; não misture DLLs de pacotes diferentes.

O destino padrão de download é `D:\models\huggingface`. Para usar uma pasta disponível em outro computador, na raiz do projeto:

```powershell
$env:NEXA_MODELS_DIR = Join-Path $env:USERPROFILE 'models'
New-Item -ItemType Directory -Force -Path $env:NEXA_MODELS_DIR
npm run desktop
```

O comando constrói o frontend e abre o Electron, que inicia seu backend em uma porta local livre. O motor local é necessário para GGUF; provedores externos são configurados na interface.

## Navegador

Em um terminal com o destino de modelos configurado, execute `npm --prefix backend start`. Em outro, execute `npm --prefix frontend run dev -- --host 127.0.0.1`.

Abra o endereço informado pelo Vite, normalmente `http://localhost:5173`. O proxy aponta para `http://localhost:3001`; `NEXA_BACKEND_URL` permite ajustá-lo. O script `iniciar-nexa.bat` contém caminhos específicos de outra máquina e não é um instalador universal.

## Verificar e empacotar

```powershell
npm --prefix backend test
npm run build:frontend
```

Para alterações no agente, teste também um pedido pequeno com um modelo real e confira arquivos e ações. Testes automatizados não comprovam sozinhos a execução de uma tarefa completa.

Após preparar o motor e validar o desktop, `npm run package:win` gera o instalador NSIS em `dist/`. O manifesto inclui `llama.cpp/bin/**`; verifique a presença e o funcionamento do motor e valide em máquina limpa antes de publicar. Não distribua conversas ou credenciais.

## Linguagem experimental

`rust-toolchain.toml` seleciona Rust stable GNU para Windows e exige linker compatível. A configuração local `.cargo/config.toml` não é versionada.

```powershell
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Esses comandos não foram executados nesta revisão documental. Consulte a [auditoria histórica](PROJECT_CURRENT_STATE.md) para limites registrados da toolchain.

O `.gitignore` mantém builds, dependências, modelos e dados privados fora do Git, preservando-os no disco. Revise `git status` e `git diff --cached` antes de enviar alterações.
