# As abas de volta depois de reiniciar o computador

O keepd guarda os shells de todas as abas; quando o computador reinicia, ele
morre com eles. O Keep traz as abas de volta sozinho (crate `keep-restauro`,
`keep restauro …`):

- os mesmos workspaces, cada aba na pasta em que estava;
- o painel ao lado da aba de que ele era painel, do mesmo lado;
- o nome que a pessoa deu à aba no app;
- na aba que tinha o Claude Code ou o Codex, a mesma conversa, retomada direto
  (sem cartão a aceitar), com as opções com que o programa rodava (`--model`,
  `--effort`…), a conta da aba e, num workspace com a IA presa a uma pasta, a
  mesma restrição.

O programa que rodava num terminal comum (um editor, um servidor) não é
repetido: a aba volta no shell, na pasta dele.

## Como

- **Retrato.** Enquanto o keepd vive, o app grava a cada 30 s o retrato das
  abas (`keep restauro manter`): workspace, aba, pasta, painel, tamanho, nome e,
  na aba com IA, a conversa, a conta e as opções. Fica em
  `<pasta de estado do app>/restauro/retrato.json` (no macOS,
  `~/Library/Application Support/Keep/restauro`).
- **Keepd novo.** O retrato do keepd anterior vai para `anterior.json` — salvo
  se a restauração dele não acabou: aí o `anterior.json` que ela usava fica, e
  um segundo reinício no meio não troca as abas que faltavam pelo pouco que já
  tinha voltado. Uma lista vazia não apaga um retrato com abas do mesmo keepd.
- **Restauração**, em duas fases:
  1. `keep restauro restaurar --fase=abas` — o app espera por ela ao abrir com
     um keepd vazio, antes de pôr qualquer coisa na tela (e então não cria o
     workspace padrão): recria as abas, a aba-mãe antes dos painéis dela, e
     devolve os nomes ao `names.json`.
  2. `keep restauro manter` — em segundo plano: retoma as conversas, uma de
     cada vez (cada uma espera a anterior subir, até 25 s, para várias não
     brigarem por memória), responde por ela a pergunta "confiar nesta
     pasta?" e grava o retrato novo.
- **Uma vez por keepd.** O que foi feito vai para `restaurado.json` antes de
  cada passo: uma rodada que caiu no meio continua de onde parou, nada é criado
  duas vezes, e a aba que voltou e a pessoa fechou não volta de novo. A
  conversa que já roda numa aba do keepd novo não ganha outra aba.

## Comandos

    keep restauro retrato [--print]          grava o retrato (--print: só mostra, em JSON)
    keep restauro restaurar [--fase=abas] [--seco]
    keep restauro manter                     restaura o que faltar e grava o retrato

`KEEP_RESTAURO_ESTADO` troca a pasta (o app passa a da pasta de estado dele:
um app de testes, com daemon próprio, nunca lê o retrato da pessoa).

## Testes

- `cargo test -p keep-restauro` — as regras (ordem da árvore, escolha da
  origem, giro do retrato) e, com dois daemons do próprio teste e a IA falsa
  do keep-ia (`cargo build -p keep-ia --examples`), o reinício inteiro.
- `tools/restauro-test.sh [--sabotage]` — o app de verdade (KeepDev) abrindo
  num keepd novo: as abas voltam antes do workspace padrão, e o retrato gira.
