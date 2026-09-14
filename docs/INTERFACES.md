# Interfaces do backend

[Índice](INDEX.md)

Mapa resumido das rotas presentes em 14/09/2026. A API é interna e ainda não possui compromisso de estabilidade pública.

No desktop, a porta é dinâmica e o cabeçalho de autenticação é `x-nexa-token`, gerenciado pelo aplicativo. Não é JWT. No standalone, a porta padrão é 3001 e a proteção depende do token configurado; esse modo não deve ser exposto como serviço público.

| Método e caminho | Finalidade |
|---|---|
| GET /api/chat/status | Estado dos provedores. |
| GET /api/chat/models | Modelos para o chat. |
| GET /api/models | Inventário de modelos. |
| POST /api/models/switch | Trocar modelo. |
| GET /api/models/store | Catálogo remoto, com filtros q, sort e limit. |
| GET /api/models/store/local | Modelos baixados e diretório. |
| POST /api/models/store/download | Download com modelId e filename. |
| GET /api/models/store/download/progress | Progresso dos downloads. |
| GET /api/conversations | Listar conversas. |
| POST /api/conversations | Criar conversa. |
| POST /api/conversations/:id/messages | Enviar mensagem. |
| GET /api/conversations/:id/activity | Atividade registrada. |
| POST /api/conversations/:id/memory | Atualizar memória. |
| POST /api/conversations/:id/cancel | Solicitar cancelamento. |
| GET /api/skills | Listar skills. |

Consulte os handlers em backend/src/server.js e backend/src/routes/ para formatos completos. Rotas legadas de execução direta e escrita foram desativadas; as mudanças do agente passam pelo fluxo de ações da conversa.
