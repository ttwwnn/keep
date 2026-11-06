---
name: pr-agent
description: Revisa um PR ou as mudanças de um ramo com o PR-Agent pela assinatura do Claude Code (claude -p), sem chave de API e sem publicar nada no GitHub. Use antes de fazer o merge de um PR que você abriu e quando o usuário pedir revisão pelo PR-Agent, sugestões de código ou a descrição de um PR. O botão "PR-Agent" do rodapé do Keep liga e desliga esta skill.
---

<!-- Instalada e mantida pelo Keep (keep ia automacao pr-agent): o Keep regrava esta pasta quando muda. -->

# PR-Agent

O `pr-agent.sh` desta pasta monta o diff, entrega-o ao PR-Agent (github.com/the-pr-agent/pr-agent, com o provider
`claude-code/` de github.com/ttwwnn/pr-agent-claude) e imprime o resultado em Markdown. O modelo responde pela conta
do Claude Code de quem roda, em `claude-opus-5-5` com esforço `medium`.

## Comandos

Rode na pasta do repositório, com prazo de 15 minutos (`timeout` de 900000 no Bash): leva de 30 s a alguns minutos.

- `sh <esta pasta>/pr-agent.sh revisar`: o ramo atual contra o ramo principal do `origin`, com o que ainda não foi
  commitado (arquivo novo entra depois do `git add`).
- `sh <esta pasta>/pr-agent.sh revisar --pr 123` (ou a URL do PR): o diff do PR, lido pelo `gh`. Nada é publicado.
- `descrever`: título e descrição para o PR.
- `melhorar`: sugestões de código.
- `perguntar "texto"`: uma pergunta sobre as mudanças.
- Opções: `--base <ref>`, `--modelo <id>` (`claude-fable-5-1`, `claude-sonnet-5-5`…) e
  `--esforco low|medium|high|xhigh|max`.

## Antes do merge

1. Com o push feito e o PR aberto, rode `revisar --pr <número>`.
2. Avalie cada apontamento pelo mérito: corrija o defeito real e descarte, com o motivo, o que não procede. O
   PR-Agent não é determinístico e pode repetir um apontamento já resolvido.
3. Arquivo listado em `files_not_reviewed` ou `files_partially_reviewed` (a cobertura, no fim da saída) ficou fora
   do orçamento de tokens: revise-o você mesmo.
4. Depois de corrigir, rode de novo; faça o merge quando não restar apontamento real.

Repositório que já tem o PR-Agent no GitHub como condição do merge dispensa esta revisão: lá, quem decide é o robô.
