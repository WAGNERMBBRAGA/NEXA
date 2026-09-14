# Plano: Project Manager - NEXA

## Visão Geral
O NEXA poderá carregar projetos de pastas do computador, examinar a estrutura, ler/escrever arquivos, e criar projetos do zero. A IA edita arquivos direto no disco.

## Arquitetura

```
Frontend (sidebar)          Backend (Express)           Disco
┌─────────────┐            ┌──────────────┐           ┌────────┐
│ Project     │──GET──────>│ /api/projects│──read───>│ .gguf  │
│ Panel       │            │ /load        │          │ .js    │
│             │──GET──────>│ /api/projects│──read───>│ .py    │
│ File Tree   │            │ /file        │          │ etc    │
│             │──POST─────>│ /api/projects│──write─>│        │
│ Actions     │            │ /write       │          │        │
│             │──POST─────>│ /api/projects│──mkdir─>│        │
│             │            │ /create      │          │        │
└─────────────┘            └──────────────┘           └────────┘
```

## Tarefas

### Fase 1: Backend - Project Service
- [ ] 1.1 Criar `backend/src/services/projectService.js`
  - `loadProject(folderPath)` — escaneia pasta recursivamente, retorna árvore de arquivos
  - `readFile(filePath)` — lê conteúdo de um arquivo
  - `writeFile(filePath, content)` — escreve/modifica arquivo no disco
  - `createProject(folderPath, structure)` — cria pasta e arquivos iniciais
  - `getProjectInfo(folderPath)` — retorna stats (total arquivos, linguagens, tamanho)
  - Ignorar: node_modules, .git, __pycache__, dist, build, .env

### Fase 2: Backend - Rotas API
- [ ] 2.1 Criar `backend/src/routes/projects.js`
  - `POST /api/projects/load` — carrega projeto { path } → árvore de arquivos
  - `GET /api/projects/file?path=...` — lê arquivo
  - `POST /api/projects/write` — escreve { path, content }
  - `POST /api/projects/create` — cria projeto { path, files: [{name, content}] }
  - `GET /api/projects/info?path=...` — stats do projeto

- [ ] 2.2 Registrar rotas no server.js
  - Adicionar `app.use('/api/projects', projectsRoutes)`

### Fase 3: Frontend - UI do Project Panel
- [ ] 3.1 Adicionar HTML do painel na sidebar (index.html)
  - Seção "PROJETO" abaixo de "MODELO"
  - Botão "Abrir Projeto" (file picker webkitdirectory)
  - Botão "Criar Projeto"
  - Árvore de arquivos (div#project-tree)
  - Info do projeto (stats)

- [ ] 3.2 CSS do painel (styles.css)
  - Estilo da árvore de arquivos (indentação, ícones por tipo)
  - Botões de ação
  - estados: vazio, carregando, com projeto

### Fase 4: Frontend - JavaScript
- [ ] 4.1 Lógica de projeto (app.js)
  - `openProject()` — abre file picker, chama POST /load, renderiza árvore
  - `renderFileTree(tree, container)` — renderiza recursivamente com ícones
  - `openFile(path)` — busca conteúdo, exibe no painel de chat
  - `saveFile(path, content)` — salva via POST /write
  - `createProject()` — modal para nome + templates, chama POST /create

- [ ] 4.2 Integração com Chat
  - Quando projeto está carregado, contexto da IA inclui: estrutura + conteúdo dos arquivos relevantes
  - AI pode solicitar: "ler arquivo X", "modificar arquivo Y", "criar arquivo Z"
  - Backend executa automaticamente as ações de arquivo

### Fase 5: IA com Acesso ao Projeto
- [ ] 5.1 Modificar aiService.js
  - Quando há projeto ativo, injetar no system prompt: "Você tem acesso ao projeto [nome]. Estrutura: [tree]"
  - Adicionar ferramentas: `read_file(path)`, `write_file(path, content)`, `list_files()`
  - Parser de respostas da IA para detectar ações de arquivo e executar

### Fase 6: Templates de Projeto
- [ ] 6.1 Criar templates predefinidos
  - HTML/CSS/JS (index.html + style.css + script.js)
  - React (package.json + src/)
  - Python (main.py + requirements.txt)
  - Node.js (package.json + index.js)
  - Em branco (só pasta vazia)

## Checkpoint
- [ ] Testar: carregar projeto existente, ver árvore, ler arquivo
- [ ] Testar: criar projeto do zero com template
- [ ] Testar: IA lê e modifica arquivos via chat
