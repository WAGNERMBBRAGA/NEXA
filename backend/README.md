# Backend do NEXA AI

API Express para conversas, projetos, modelos e ações do agente.

Consulte o [guia de desenvolvimento](../docs/DEVELOPER_GUIDE.md) para preparar dependências e destino de modelos. Na raiz, use `npm --prefix backend ci` e `npm --prefix backend start`. Para testes: `npm --prefix backend test`.

A porta standalone padrão é 3001. O desktop inicia seu backend em uma porta local livre. Ollama não é requisito para o fluxo GGUF com llama.cpp integrado.

- [Arquitetura](../docs/ARCHITECTURE.md)
- [Interfaces](../docs/INTERFACES.md)
- [Guia do usuário](../docs/GUIA_DO_USUARIO.md)
