# Arquitetura do NEXA

[Índice](INDEX.md)

| Área | Responsabilidade |
|---|---|
| desktop/main.cjs | Janela Electron, backend local e dados do aplicativo. |
| frontend/ | Interface JavaScript, HTML e CSS, construída com Vite. |
| backend/src/server.js | API Express e coordenação das conversas. |
| backend/src/services/ | Agente, ações, contexto, modelos e downloads. |
| backend/src/config/ | Configuração, caminhos e persistência. |
| llama.cpp/bin/ | Motor preparado para empacotamento, não versionado. |

Fluxo principal: interface → backend → modelo escolhido → ações interpretadas e validadas → projeto e resultados da conversa.

No desktop, o backend escuta em 127.0.0.1 numa porta escolhida na inicialização. A API exige um token efêmero compartilhado com a janela. Os dados seguem a pasta userData do Electron. No standalone, a raiz padrão é o backend, substituível por NEXA_USER_DATA.

Downloads usam configuração própria: NEXA_MODELS_DIR, com padrão atual em D:\models\huggingface. Isso ainda limita a portabilidade.

## Linguagem

compiler/ contém o compilador; runtime/ contém execução e contratos; cli/ e tooling/ contêm ferramentas; cts/ reúne testes; spec/ contém especificação em evolução.

O aplicativo e a toolchain compartilham o repositório, mas isso não comprova integração completa entre ambos. Consulte a [visão de evolução](VISAO_E_EVOLUCAO.md).
