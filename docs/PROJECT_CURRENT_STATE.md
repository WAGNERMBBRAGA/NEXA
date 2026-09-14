# NEXA — estado atual do repositório

Atualizado em 06/09/2026. Este documento descreve o que existe hoje no
repositório; não é uma promessa de funcionalidades futuras.

## Identidade atual

O diretório reúne dois produtos que usam o nome **NEXA**:

1. **Toolchain da linguagem NEXA** — compilador e runtime em Rust, com fonte
   `.nexa`, pipeline semântico, NIR, backend WASM, host WASM e CLI.
2. **NEXA Local Assistant** — aplicação local de assistência por IA, com
   frontend Vite/JavaScript e backend Node/Express. Ela gerencia conversas,
   modelos locais, projetos e skills armazenadas em arquivos.

Eles compartilham a raiz, scripts de inicialização e documentação. Não há
hoje um contrato de integração entre o assistente web e o compilador Rust.

## Estrutura mantida

| Área | Papel atual |
|---|---|
| `compiler/` | Frontend da linguagem, análise semântica, NIR, lowering, backend WASM, pacote e linker. |
| `runtime/` | Host WASM e contratos de recursos, segurança, IA, provedores, tarefas e agentes. |
| `tooling/` e `cli/` | CLI `nexa`, formatter, LSP, protocolo, base semântica e test runner. |
| `cts/` | Casos de conformidade de lexer, parser, resolver, checker, fluxo, efeitos, pacote e codegen. |
| `backend/` e `frontend/` | Assistente local de IA, separado da toolchain Rust. |
| `spec/` | Apenas esqueletos de registries, requisitos e gramática; não é ainda a fonte normativa completa. |

Há 54 crates declarados no workspace e 56 manifests Rust sob as áreas de
implementação. Os dois fora do workspace são `compiler/` (`bump_allocator`,
utilitário isolado) e `compiler/nexa-build` (grafo de build/link/cache ainda
não integrado).

## Toolchain Rust

### O que está implementado e conectado

- Lexer, parser, AST/CST, resolução de nomes e type checking possuem suites
  CTS próprias.
- O caminho executável compila um subconjunto para WASM e o host próprio roda
  casos de funções, chamadas, condicionais, loops, `break`/`continue`,
  operações inteiras verificadas, strings estáticas, `Console::write` e arrays
  locais.
- Há CTS de codegen com 21 casos positivos e 9 casos de trap. Os casos com
  golden de saída são executados pelo host WASM durante a suite de codegen.
- Existem modelos para ownership, borrow, recursos, tarefas, efeitos,
  contratos, pacotes, lockfiles, ABI e LSP.

### Limites atuais

- O carregamento de metadata do workspace foi restaurado em 05/09/2026 com a
  remoção de uma dependência `serde_derive.workspace` inválida e não usada em
  `nxr-ai`. Após sincronizar as dependências já declaradas no lockfile,
  `cargo fmt --check`, `cargo test --workspace` e
  `cargo clippy --workspace --all-targets -- -D warnings` passaram.
- Estruturas, campos, tuplas reais, strings mutáveis e coleções compostas
  passam parcialmente pelas camadas semânticas, mas não fazem parte do perfil
  executável. O lowering rejeita structs/campos e vários usos de arrays.
- O backend gera `Bool` como `i64`, enquanto a ABI pública mapeia `Bool` para
  `i32`. Isso precisa ser unificado antes de qualquer ABI externa.
- O linker gera um módulo WASM válido de metadados de símbolos; ele não une
  seções executáveis de pacotes.
- `nexa test` descobre anotações `@test`, mas no estado atual não executa o
  corpo do teste: uma assinatura válida sem erro de parse é marcada como
  aprovada.
- `nexa package resolve` usa um registro em memória vazio. `restore` apenas
  valida lockfile e dependências locais; não baixa nem valida artefatos de
  registry.
- `nexa-build` contém o início de build reproduzível, mas está fora do
  workspace e não é chamado pela CLI.
- Fluxo assíncrono real, generators, concorrência, bloqueio/yield e providers
  de IA reais ainda têm pontos explicitamente pendentes.

## NEXA Local Assistant (desktop)

### O que está implementado

- Aplicativo Electron com backend Express interno em uma porta livre escolhida
  a cada execução e integração com `llama-server` local na porta 8080.
- Conversas, memória por conversa, skills em JSON, árvore de projeto,
  edição/criação de arquivos, seleção de modelo, busca no Hugging Face e
  download de GGUF.
- Cada conversa mantém seu próprio projeto vinculado, memória, mensagens e
  resultados verificáveis. Ao reabrir o aplicativo, a conversa ativa e o
  projeto correspondente são restaurados juntos.
- O frontend usa os endpoints expostos pelo servidor atual; a sintaxe de todos
  os arquivos JavaScript mantidos foi validada com `node --check`.
- Quando nenhum modelo de chat está instalado, a tela inicial oferece ações
  diretas para instalar um GGUF ou conectar um provedor externo.

### Segurança e persistência do aplicativo desktop

- O backend desktop escuta exclusivamente em `127.0.0.1` e todas as rotas
  `/api` exigem um segredo efêmero de 256 bits compartilhado somente com a
  janela Electron da instância ativa.
- Configuração, conversas, memória, sessão ativa e índices vetoriais são
  gravados sob `app.getPath('userData')`, fora dos recursos instalados.
- O pacote exclui `backend/data`, `backend/config.json` e testes internos. O
  instalador não distribui conversas nem trechos de projetos analisados.
- O desktop usa bloqueio de instância única e aguarda uma resposta autenticada
  do backend antes de abrir a janela. A porta dinâmica evita colisão com uma
  instância antiga ou outro programa local.
- Chaves de provedores são mantidas em memória durante a execução e, no desktop,
  criptografadas pelo `safeStorage` do Electron. O `config.json` persiste apenas
  URL, modelo e preferências sem segredos.
- Downloads aceitam somente `organização/modelo` e um nome simples `.gguf`; o
  destino é confinado à pasta de modelos.
- Desenvolvimento e pacote usam Express 5.2.1 e CORS 2.8.6.
- Metadados remotos da loja de modelos são escapados antes de entrar na
  interface e downloads concluídos reaparecem no seletor de modelos.
- O encerramento para apenas os servidores de IA iniciados pelo próprio NEXA;
  processos externos do usuário não são finalizados.
- Uma continuação de correção usa os achados estruturais da etapa anterior. Em
  projetos Python sem testes e com `requirements.txt` divergentes, o NEXA cria
  uma cobertura inicial de compilação dos pontos de entrada e um guia de
  escopo das dependências, mesmo se o modelo local não emitir ações.
- Perguntas de confirmação, como “você corrigiu?”, usam o registro verificável
  desta conversa: o NEXA informa os arquivos realmente alterados sem pedir que
  o modelo repita a resposta anterior. Após o guia de dependências ser criado,
  a inspeção deixa de tratar manifestos de escopos documentados como conflito.

### Limites conhecidos

- Os antigos endpoints `/api/exec` e `/api/exec/batch` foram desativados em
  05/09/2026. A execução de desenvolvimento passa pelo protocolo validado de
  ações da conversa; ainda falta autenticação local para uma distribuição em
  rede.
- As rotas legadas `/api/projects/write` e `/api/projects/create` também foram
  desativadas. Assim, a criação e edição feitas pelo assistente não contornam
  a validação de caminho do protocolo `NEXA_ACTIONS`.
- As rotas de navegação aceitam caminhos escolhidos pelo usuário porque o NEXA
  precisa abrir projetos locais. Elas ficam protegidas pelo segredo da instância
  e o servidor não é exposto à rede.
- A pilha antiga de IA, que não era montada e continha respostas simuladas, foi
  removida. `services/aiService.js` é a única integração de chat mantida.
- No modo web de desenvolvimento, sem Electron, chaves fornecidas funcionam
  apenas durante o processo atual; persistência criptografada é exclusiva do
  aplicativo desktop.
- O assistente coloca conteúdo de arquivos e skills diretamente no prompt do
  modelo. Sem delimitação forte de conteúdo não confiável e sem aprovação de
  ferramentas, há risco de prompt injection induzir ações locais perigosas.

### Protocolo de ações verificáveis

Em 05/09/2026 foi introduzido o protocolo `NEXA_ACTIONS` para o fluxo de
conversas. O modelo precisa devolver ações JSON estruturadas para escrever
arquivos, criar um projeto sob a raiz atual ou rodar comandos de
desenvolvimento permitidos. O backend valida os caminhos contra a raiz do
projeto, não usa shell para esses comandos e devolve o resultado efetivo de
cada ação. Uma resposta que alegue correção sem ações estruturadas recebe um
aviso explícito de que nada foi aplicado. Isso substitui a confiança em texto
livre. A interface compilada também carrega `app.js` como módulo, garantindo
que a lógica do assistente esteja presente no pacote de produção.

O fluxo agora permite até três etapas automatizadas por solicitação: o modelo
propõe ações, o NEXA as executa, fornece o resultado estruturado à próxima
etapa do modelo e só então apresenta a conclusão. Há testes automatizados
para esse ciclo, para o confinamento de arquivos e para o bloqueio de comandos
fora da política. A qualidade das decisões ainda é limitada pelo modelo de IA
configurado; o protocolo garante que as afirmações sobre alterações tenham
evidência, não que o modelo sempre escolha a alteração ideal.

O contexto enviado ao modelo passou a priorizar os arquivos citados no pedido
e arquivos de entrada do projeto (por exemplo, `package.json` e `Cargo.toml`),
em vez de usar apenas os primeiros arquivos encontrados. A interface atualiza
a árvore do projeto depois de ações de criação ou gravação concluídas e os
controles dinâmicos continuam disponíveis no bundle de produção.

O agente também pode solicitar `read_file` durante o ciclo: a leitura exige um
caminho relativo, permanece confinada ao projeto selecionado, rejeita binários
e limita o conteúdo devolvido ao modelo. Isso permite inspecionar um arquivo
antes de alterá-lo sem reabrir os endpoints legados de escrita direta.

Gravações feitas por `write_file` são atômicas: o conteúdo é preparado em um
arquivo temporário na mesma pasta e só então substitui o destino. O
confinamento de caminhos também verifica o caminho físico do ancestral
existente, bloqueando links simbólicos que levem para fora do projeto.

Arquivos incluídos no contexto e resultados de ações agora são delimitados e
identificados como dados não confiáveis no prompt do agente. O modelo é
instruído a não obedecer comandos, instruções ou tentativas de alteração do
protocolo que apareçam dentro desses conteúdos.

Resultados compactados das ações são persistidos junto à resposta da conversa.
Eles reaparecem na interface ao reabrir o histórico e podem informar a próxima
mensagem do agente, sem armazenar saídas de comando sem limite.

O inicializador `iniciar-nexa.bat` agora verifica a presença de Node.js, npm,
llama-server, modelo GGUF e dependências antes de iniciar os processos, e usa
diretórios de trabalho explícitos para backend e frontend. Falhas de comandos
executados pelo agente informam o código de saída e uma parte limitada do
diagnóstico na conversa.

Em 05/09/2026, o caminho antigo do modelo no inicializador foi substituído pelo
Qwen 3.5 4B disponível em `D:\models`. A instância atual foi verificada com
sucesso nas portas da IA (8080), backend (3001) e interface (5173).

## Operação atual

O desenvolvimento ainda pode ser iniciado pelos scripts locais. A distribuição
desktop inicia o backend e, quando necessário, o `llama-server` sem exigir
terminal. O Rust usa toolchain GNU e um `target-dir` ASCII por limitação
conhecida do linker com o caminho do projeto.

## Critério de liberação do instalador desktop

O instalador final permanece bloqueado enquanto houver falha em qualquer gate
do aplicativo: testes do backend, build do frontend, auditoria de dependências,
inicialização da versão descompactada, carregamento dos elementos da janela e
fluxo conversa-projeto-auditoria. Limitações da toolchain Rust são controladas
separadamente porque ela ainda não é distribuída pelo instalador do assistente.

## Prioridades para tornar o projeto consistente

1. Manter o assistente desktop e a toolchain Rust como entregas explicitamente
   separadas até existir um contrato de integração entre elas.
2. Substituir o test runner declarativo da linguagem por compilação e execução
   real.
3. Alinhar ABI e backend, integrar `nexa-build`, implementar linkagem de
   código e resolver/restore de registry reais.
4. Definir requisitos e spec normativos, CI, testes de segurança, fuzzing,
   licença e processo de release antes de distribuição.

## Evidência da auditoria

Foram inventariados código, manifests, testes/CTS, documentação, scripts e
configuração mantidos. Dependências vendorizadas (`node_modules`), artefatos
de build (`target`, `llama.cpp-build`) e binários foram excluídos da leitura de
implementação, pois não são código autoral do NEXA. A metadata, a formatação,
os testes globais e Clippy estrito do Rust foram validados. JavaScript mantido
passou na verificação sintática.
