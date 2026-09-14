# Guia do usuário — NEXA AI

[Índice](INDEX.md) · [Resolver problemas](SOLUCAO_DE_PROBLEMAS.md)

## Disponibilidade e preparação

O fluxo simplificado usa o aplicativo desktop completo para Windows, empacotado com llama.cpp. Não é necessário instalar Ollama ou LM Studio para esse fluxo. Em 14/09/2026 não havia instalador publicado nas [Releases](https://github.com/WAGNERMBBRAGA/NEXA/releases); consulte essa página para futuras versões.

Baixar o código não equivale a instalar o aplicativo. Para executar pelo código, siga o [guia de desenvolvimento](DEVELOPER_GUIDE.md). Outros sistemas operacionais ainda exigem adaptação e validação.

Você precisa de espaço para os modelos, memória suficiente para o modelo escolhido e internet para busca e download. Não existe um requisito único de RAM que garanta o funcionamento de todos os modelos.

## Baixar e escolher uma IA

1. Abra o NEXA e, quando ainda não houver modelo, use **Instalar modelo local**.
2. Na **Model Store**, busque um modelo no Hugging Face.
3. Observe o tamanho e as informações de adequação ao computador. Escolha uma variante GGUF de chat compatível.
4. Clique em **Baixar** e aguarde a conclusão.
5. No topo da conversa, selecione **NEXA Local** e escolha o modelo.
6. Aguarde o carregamento e confira o indicador de estado antes de enviar uma tarefa.

O aplicativo inicia o motor integrado e aplica sua configuração. Isso não garante suporte a toda arquitetura: embeddings e adaptadores não substituem um modelo de chat. Consulte também os termos do autor do modelo.

As indicações de desempenho são estimativas, não medições na sua máquina. A execução usa CPU por padrão; detectar uma GPU não significa utilizá-la automaticamente.

## Trabalhar em um projeto

1. Clique em **+ Nova conversa**.
2. Use **Abrir projeto nesta conversa** e selecione a pasta desejada.
3. Confira o projeto indicado no topo do chat.
4. Se necessário, preencha **Linguagem do projeto**.
5. Descreva o objetivo, as restrições e como verificar o resultado.
6. Confira as ações registradas, os arquivos e as verificações executadas.

Exemplos:

> Analise este projeto e explique como funciona. Primeiro identifique os arquivos principais.

> Crie um cadastro de produtos em Python nesta pasta. Comece com cadastro e listagem e valide essa primeira etapa.

> Investigue este erro, corrija a causa e execute os testes relacionados. Explique o que foi alterado.

Para tarefas grandes, avance por etapas. Mantenha uma cópia ou commit antes de mudanças importantes: o agente trabalha sobre arquivos reais. Uma resposta dizendo que terminou não comprova que os arquivos foram criados ou os testes executados.

## Memória e skills

Em **Memória desta conversa**, registre objetivos, padrões e restrições. Revise e salve o conteúdo; não armazene senhas ou tokens ali.

Em **Skills ativas**, selecione as instruções especializadas úteis à conversa. Elas orientam o trabalho, mas não eliminam os limites do modelo.

## Provedores externos e privacidade

Use **Conectar provedor** ou **Configurações da IA** para configurar uma API externa. Nesse modo, as solicitações e o contexto enviados ao provedor deixam de ser processados exclusivamente no computador.

Com um modelo local disponível, a inferência ocorre no computador. Busca, download e outras ações que dependam de serviços externos ainda usam a rede.

No desktop, configuração, conversas e memória ficam na pasta de dados do aplicativo, separadas dos recursos instalados. O diagnóstico é registrado em `backend.log` nessa pasta. Revise logs antes de compartilhá-los.

## Participar

O NEXA é recente e pode falhar ou não concluir uma tarefa. Relate sua experiência seguindo [Como contribuir](../CONTRIBUTING.md).
