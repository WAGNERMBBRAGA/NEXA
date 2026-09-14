# Solução de problemas

[Índice](INDEX.md) · [Guia do usuário](GUIA_DO_USUARIO.md)

| Situação | Verificação |
|---|---|
| Nenhuma IA disponível | Baixe um GGUF de chat e selecione-o no topo da conversa. |
| Download falha | Confira internet, espaço e permissão no destino. Autenticação para modelos privados ou restritos não está documentada nesse fluxo. |
| Modelo não carrega | Confira download completo, memória disponível e arquitetura compatível com o motor. |
| Resposta lenta | Confira RAM livre e tamanho do modelo. CPU é o padrão; desempenho exibido é estimativa. |
| Responde sem criar arquivos | Confira projeto e ações registradas. Reduza o pedido a uma etapa; a emissão de ações depende do modelo. |
| Projeto errado | Confira a conversa e use Abrir projeto nesta conversa. |
| Motor não encontrado | O clone não inclui binários; siga o guia de desenvolvimento. |
| Unidade D: inexistente | Defina NEXA_MODELS_DIR conforme o guia de desenvolvimento. |

## Pasta dos modelos

O padrão atual é `D:\models\huggingface`. O [guia de desenvolvimento](DEVELOPER_GUIDE.md) mostra como usar a pasta models do usuário, que também faz parte das raízes de descoberta.

Para o aplicativo instalado, feche o NEXA e inicie seu executável na mesma sessão PowerShell após definir NEXA_MODELS_DIR, usando o caminho real da instalação.

## Relatar um problema

Informe versão, sistema, hardware, nome completo do modelo e variante GGUF, passos e erro observado. Use um projeto mínimo sem dados privados. O desktop grava backend.log na pasta de dados do aplicativo; em desenvolvimento acompanhe o terminal.

Revise logs antes de anexá-los a uma [issue](https://github.com/WAGNERMBBRAGA/NEXA/issues). Não publique tokens ou conversas privadas.
