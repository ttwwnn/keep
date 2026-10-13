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

# Conversa nova numa aba, e os links `claude-cli://`

`keep nova` abre uma conversa nova do Claude numa aba nova (no lugar do
`claude-nova-aba` do kit-mac): é o caminho para uma sessão abrir outra, e o
Terminal do macOS só a pessoa abre. A aba nasce do keepd, com o ambiente limpo
dele, na pasta pedida; o `claude` é digitado na aba, e a função `claude` do
shell (o esforço padrão, a pasta presa do workspace) vale.

    keep nova --workspace W --pasta P --passagem ARQUIVO     lê o arquivo inteiro e segue
    keep nova --workspace W --pasta P --pedido "texto"       o texto vai para ~/.claude/handoffs/abas/
    keep nova [--workspace W] --pasta P --argumentos -- ARG…  claude ARG… (sem workspace: o da última aba usada)
        [--nome "rótulo"] [--sem-enter]
    keep nova --listar

`keep links instalar | conferir` monta `~/Applications/Ghostty.app`, por onde o
Claude Code abre os links `claude-cli://` (ele escolhe o terminal pelo
`TERM_PROGRAM`, e o Keep diz `ghostty`): o executável é uma cópia do `keep`, que,
chamado com o nome "Ghostty", abre o link com `keep nova --argumentos` (com o Keep
fechado, abre o Keep e tenta de novo; log em `~/Library/Logs/desvio-deep-link.log`).
O app o monta, ou o põe em dia, a cada abertura (só o app de produção; um app de
testes nunca); com o Ghostty de verdade em `/Applications`, não. Substitui o
desvio que o kit-mac montava (lançador em C + script).

Testes: `cargo test -p keep-restauro` (o `tests/nova.rs` abre conversas de
verdade num daemon do próprio teste, com a IA falsa como `claude`).

# A conversa que parou numa queda continua sozinha

Quando o login cai, a rede some ou o limite de uso estoura no meio de um
trabalho, o Claude Code grava uma fala de erro e a conversa PARA ali, mesmo
depois de a causa passar. O app roda `keep continuar` a cada 15 s: a conversa
cuja última fala é esse erro recebe "continue: …; siga exatamente de onde
parou." quando a causa volta.

- Quem diz que parou é o transcrito (última fala da conversa principal, depois
  da abertura do processo); a sessão estacionada em segundo plano conta pela
  sessão `bg`.
- A porta de cada causa: login, a credencial da aba com acesso válido (aba
  presa a uma conta fica de fora: o login dela volta pelo menu); rede, a API
  responde; limite, a conta da aba tem folga.
- A tela só confirma que é seguro: caixa vazia (ou já com a mensagem, faltando
  o Enter), nada rodando, nenhuma tarefa do rodapé selecionada, o erro à vista.
  Vista desviada (modo transcrito, painel de agentes, diálogo do limite,
  conversa rolada) volta à conversa só com a pessoa longe do teclado há 2 min.
- No máximo 3 vezes seguidas sem a conversa andar, 2 min entre elas; a mensagem
  tem de chegar ao transcrito para contar.
- Estado e registro em `continuar.json` e `continuar.log`, na pasta do restauro.

```
keep continuar             varre e destrava (o app)
keep continuar --seco      diz o que faria
keep continuar --status    o que vê em cada conversa e as três portas
```

Teste: `cargo build -p keep-restauro --examples && cargo test -p keep-restauro
--test continuar` (daemon do teste, Claude parado de mentira em quatro abas).
Era o `cc-continuar` do kit-mac até 13/10/2026.
