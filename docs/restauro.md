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

## No Windows

O app do Windows roda os mesmos comandos, nos mesmos relógios do Mac, e não recria aba nenhuma por conta
própria (antes ele guardava um retrato no `state.json` e abria as abas em JavaScript, sem as conversas):

- ao abrir, e quando o keepd volta depois de uma ausência (a janela espera até 1 min por um keepd lento,
  em vez de tomar a lista vazia por verdade), um keepd sem abas é perguntado, uma vez por keepd, por
  `restaurar --fase=abas`, antes de a janela mostrar o vazio ou podar a ordem da barra lateral e os nomes;
  o aviso "Restaurado após reinício" fica;
- logo ao abrir e a cada 30 s, `manter` (as conversas, uma de cada vez, e o retrato novo), e a cada 15 s
  `continuar`; cada um roda sozinho, um de cada vez, enquanto o keepd responde;
- o estado é `%APPDATA%\Keep\restauro` (ou `restauro` dentro de `KEEP_STATE_DIR`, num app de testes): o
  app passa `KEEP_STATE_DIR` e `KEEP_RESTAURO_ESTADO` a cada chamada, e o keep que roda dentro de uma aba
  chega ao mesmo lugar sem variável nenhuma;
- o que o app guarda por número de aba (a ordem em que as abas estão na barra e na faixa, a última de cada
  workspace, a que está à frente) é de um keepd só: o `restaurar --fase=abas` diz as `criadas` (o número
  que cada aba refeita tinha no keepd do retrato e o que tem agora) e o app leva esses três para os números
  novos, em vez de zerá-los; o que era de uma aba que não voltou vai com ela. Os workspaces guardados
  (`hidden`) valem por nome e seguem guardados: a poda dos que não rodam só acontece depois do restauro. O
  workspace padrão com o nome da pessoa só nasce se nada voltou;
- a identidade do keepd, para o app saber que o daemon é outro, é `<pid>-<início em ms>` com o início que
  ele mesmo diz (`List3`), e os nomes que a pessoa deu às abas valem para um keepd só, porque os números
  das abas recomeçam em 1: ao mudar o keepd o app esquece os nomes do anterior, e os das abas que voltam
  chegam de volta pelo `names.json`;
- os nomes: o `names.json` é o do Mac (`{"daemonStart": <segundos>, "tabs": {"<workspace>␟<aba>": nome},
  "workspaces": {…}}`), na pasta do Keep (`%APPDATA%\Keep`, ou `KEEP_STATE_DIR`). O app grava nele os nomes que a pessoa dá e adota os que o
  núcleo põe (as abas que o restauro refez, `keep nova --nome`), a cada 2 s; no Windows, onde o pipe não
  tem nascimento, o `daemonStart` é o início que o próprio keepd diz. A troca do nome pelo núcleo e a do
  app passam pela mesma trava, `.state.lock` (`LockFileEx` no Windows, `flock` no Mac);
- a trava entre processos do crate (`.restaurar.lock`, `.retrato.lock`, `.continuar.lock`) usa o
  bloqueio de arquivo da biblioteca padrão nos dois sistemas; no Windows ela não travava nada;
- o "pessoa longe do teclado" do `continuar` é o `GetLastInputInfo` (o `ioreg` do Mac), e sem leitura vale 0
  (a pessoa está por perto);
- depois da restauração a varredura de worktrees órfãs respeita o `restaurado.json` (a conversa devida
  não é órfã) porque o app e o núcleo olham a mesma pasta.

O que ainda depende do Windows de verdade: o `GetLastInputInfo`, o `LockFileEx` e a subida das conversas no
PowerShell compilam (`cargo check --target x86_64-pc-windows-msvc`) mas só rodam no CI; o que foi visto
rodando é o mesmo fluxo no app com um keepd de teste no Mac (keepd morto: as abas e o nome voltam com o
aviso; app aberto sobre um keepd morto: o mesmo).

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

## No Windows

`keep nova` é o mesmo comando (`keep.exe`) e já mostra a aba no app:

- a aba abre no keepd, na pasta pedida — sem o prefixo `\\?\` que o `canonicalize` do Windows poria na
  pasta e no arquivo do texto digitado —, e a linha é escrita na sintaxe do PowerShell da aba;
- `--nome` vale: o nome vai para o `names.json` (acima) e o app o adota em até 2 s;
- o `keep` está no fim do PATH de toda aba que o keepd abre (e em `KEEP_IA_BIN`): uma sessão do Claude
  numa aba chama `keep nova` sem caminho. Fora de uma aba o `keep` não está no PATH: o instalador não
  mexe nele (editar o PATH do usuário num instalador NSIS é o tipo de coisa que não se prova daqui);
- os links `claude-cli://` abrem em aba do Keep no Windows **se a pessoa ligar** (Configurações, "Links do Claude
  Code", ou `keep links instalar`). Desligados, o Claude Code segue como sempre (abaixo).

## Os links `claude-cli://` no Windows

O que o Claude Code 2.1.296 faz lá (lido no executável, que traz o JavaScript em texto):

- registra o protocolo em `HKCU\Software\Classes\claude-cli` (`reg add`: `(Padrão)` = `URL:claude-cli`, `URL Protocol`
  vazio, `shell\open\command` = `"<claude.exe>" --handle-uri "%1"`) e, **em toda sessão**, lê o comando com `reg query`
  e o refaz se não contiver o caminho do `claude.exe` dele — a não ser que o `settings.json` tenha
  `"disableDeepLinkRegistration": "disable"`;
- o `claude --handle-uri <link>` só aceita exatamente esses dois argumentos ("extra arguments indicate argument
  injection via the URL"). Ele lê o link (`claude-cli://open?q=<texto>&cwd=<pasta>&repo=<dono/nome>`; também
  `install-plugin?plugin=…&marketplace=…`), acha a pasta (`cwd`, ou o clone local de `repo`) e abre o terminal que
  achar: o `wt.exe` do PATH (`wt.exe -d <pasta> -- <claude.exe> <args>`, com `;` escapado), senão `pwsh.exe`, senão
  `powershell.exe` (`-NoExit -Command & '<claude.exe>' …`), senão `cmd.exe /d /v:off /s /k`. Não olha `TERM_PROGRAM`
  nem `deepLinkTerminal` no Windows, de modo que não há como fazê-lo escolher o Keep pelo ambiente, como se faz no Mac
  com o `Ghostty.app`;
- o que ele passa ao `claude` no terminal: `--deep-link-origin`, `--deep-link-repo=<repo>`,
  `--deep-link-last-fetch=<ms>`, `--deep-link-cwd-b64=<base64 de URL da pasta>` e `--prefill-b64=<base64 de URL do
  texto>`. O texto só é **pré-preenchido** na caixa de entrada, nunca enviado, e vai em base64 justamente para nenhum
  shell o interpretar;
- as regras dele para o link (`u1o` e `cFr` no executável): `cwd` absoluta, sem UNC, sem `..`, sem controle nem
  caractere invisível, até 4096 caracteres; `q` até 5000 caracteres, sem controle além de quebra de linha e tabulação;
  `repo` no formato `dono/nome`.

O desvio escolhido é o menos invasivo que existe: o Keep se registra **no lugar do Claude Code** e abre a aba ele
mesmo.

- **Ligar** (`keep links instalar [--app Keep.exe]`; o item das Configurações chama o mesmo comando) faz duas coisas,
  as duas em HKCU (nunca HKLM, sem administrador) e as duas desfeitas por `keep links remover`:
  1. `HKCU\Software\Classes\claude-cli` passa a abrir `"<Keep.exe>" --open-uri "%1"`;
  2. o `settings.json` do Claude Code (`CLAUDE_CONFIG_DIR` ou `~/.claude`) ganha `"disableDeepLinkRegistration":
     "disable"`, editado no texto, sem mexer em mais nada (`membro_json.rs`), para o Claude Code não refazer o
     registro na próxima sessão.
  O que havia antes de cada um vai para `links-windows.json` (na pasta de estado do Keep, `%APPDATA%\Keep\ia`) **antes**
  de qualquer gravação. **Desligar** devolve exatamente isso, mas só o que ainda é do Keep: se o Claude Code ou outro
  programa reescreveu o comando, ou a pessoa mudou a chave do `settings.json`, aquilo já não é do Keep e fica como
  está. Quem já tinha `disable` no `settings.json` o mantém ao desligar. O instalador **não registra nada**; o
  desinstalador roda `keep links remover`, para nenhum link apontar para um programa que não existe mais. Uma
  atualização do app (o instalador por cima) não desfaz o registro, porque o caminho do `Keep.exe` não muda.
  `keep links conferir [--json]` diz o que está e o que falta (o Claude Code pode desfazer o primeiro item se o
  segundo faltar; o item das Configurações avisa e o ligar de novo refaz).
- **Abrir** (`Keep.exe --open-uri "<link>"`): o app (o que o link iniciou, ou o que já estava aberto, pelo
  single-instance) só repassa o link inteiro, como um único argumento e sem shell, a `keep links abrir --esperar 90
  <link>`, que o confere (`deeplink.rs`: esquema, ação, parâmetros, os limites acima, mais estritos no `cwd`: só
  unidade `C:\`, sem rede nem caracteres que o Windows não aceita num nome, e a pasta tem de existir, ser pasta e não
  passar por um caminho de rede) e abre a aba com o mesmo código do `keep nova`: workspace da aba usada por último,
  na pasta do link, digitando `claude --deep-link-origin [--deep-link-repo=…] [--deep-link-cwd-b64=…]
  [--prefill-b64=…]` (a função `claude` do PowerShell da aba vale). Um lançamento com qualquer outro formato
  (argumento a mais, outro esquema) é ignorado no app e recusado no núcleo. O app avisa numa caixa se não deu.
  Com o Keep ainda subindo (o link foi o que o abriu), o núcleo espera o daemon por até 90 s.
- **Política (igual à do Mac)**: sem pergunta de confirmação, porque o efeito de um link é uma aba com um texto
  pré-preenchido que a pessoa lê e manda ou apaga (e que o Claude Code marca com `--deep-link-origin`); a pasta vai
  direto ao keepd, nunca a um shell; link sem `cwd`, ou com uma pasta que não existe, abre na pasta pessoal (como o
  `Ghostty.app`), sem avisar o Claude de pasta nenhuma. Um `repo` sem `cwd` não é procurado em disco (o Claude Code
  guarda os clones que conhece; o Keep não tem esse índice): vai ao `claude` em `--deep-link-repo` e a aba abre na
  pasta pessoal. A ação `install-plugin` é recusada (instalar plugin não é papel do Keep).
- Log: `%APPDATA%\Keep\ia\links.log` (sem o texto do pedido).
- Efeito a saber: enquanto o item estiver ligado o Claude Code não refaz o registro do protocolo, e os links do
  claude.ai deixam de abrir o Windows Terminal; quem prefere o Windows Terminal desliga o item.

Testes: `cargo test -p keep-restauro` (o `deeplink.rs`, com links maliciosos; o `protocolo.rs`, sobre um registro na
memória; o `membro_json.rs`; e `tests/deeplink.rs`, o link de verdade num daemon do próprio teste, com o texto cheio
de metacaracteres de shell chegando só em base64). O registro de verdade (`windows-sys`) compila para o alvo
`x86_64-pc-windows-msvc` e é exercido pelo roteiro `apps/windows/e2e/run.ps1` no CI do Windows (ligar, lançar o link
como o registro lança, recusar os maliciosos, desligar, ligar, atualizar por cima e desinstalar).

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

# A conversa parada há 12 h sai da memória, e a aba fica

Cada Claude Code aberto pesa de 200 a 400 MB, e as abas esquecidas há um dia
somam gigabytes num Mac de 8 GB. Desde 21/10/2026 é o padrão do Keep: a
conversa parada há mais de 12 h é fechada, a aba segue no Keep com o shell, e
a conversa volta quando a aba é mostrada.

- **Adormecer** (`keep restauro adormecer`, o app 20 s depois de abrir e a cada
  5 min): sai do Claude pelo `/exit` a aba que está fora de vista em todas as
  janelas (o app passa as que mostra em `--vista W/N`), livre (caixa vazia,
  nada rodando, nenhuma pergunta, nada em segundo plano no rodapé) e cuja
  última fala (da pessoa ou do Claude) no transcrito tem mais de 12 h. O
  tráfego da aba e a data do arquivo não servem: o app redesenha a aba e o
  Remote Control grava no transcrito sem conversa nenhuma.
- **Antes de sair**, a conversa, a conta e as opções vão para
  `adormecidas.json` (por keepd, na pasta do restauro). Se a saída falha, a
  aba sai da lista e segue como estava.
- **O nome**: sem o Claude a aba perderia o título da conversa. A que não tem
  nome dado pela pessoa recebe o título como nome (`names.json`), e o nome sai
  quando ela acorda, se ninguém o trocou. O nome que a pessoa deu nunca muda.
- **Acordar** (`keep restauro acordar --workspace W --aba N`, o app ao mostrar
  a aba): a conversa volta na mesma aba, como a restauração a retoma (os
  parâmetros, a conta, a pasta presa). Não volta se a conversa já roda em
  outro lugar ou se a aba roda outra coisa: a pessoa tomou a aba. Uma aba
  posta na tela durante a rodada é acordada no fim dela.
- **Reinício**: o retrato leva a aba adormecida com a conversa (`adormecida`)
  e a restauração a recria adormecida, sem subir a conversa; ela acorda do
  mesmo jeito. A varredura de worktrees não a toma por órfã (o shell da aba
  segue com a conversa no histórico).
- **Só o Claude**: o Codex fica de pé.

```
keep restauro adormecer [--seco] [--vista W/N]…
keep restauro acordar --workspace W --aba N
```

`KEEP_ADORMECER_PARADA_S` troca as 12 h (os testes).

Testes: `cargo build -p keep-ia --examples && cargo test -p keep-restauro
--test adormecer` (daemon do teste, IA falsa em quatro abas: só a parada,
livre e fora de vista adormece; o retrato a leva; acordar a retoma; o reinício
a mantém adormecida) e `tools/adormecer-test.sh` (o KeepDev de verdade: a aba
fora de vista adormece, a da frente não, e mostrar a aba a acorda).
