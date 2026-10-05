# IA no Keep: contas, consumo, IA por aba e lixeira de worktrees

Recursos que o Keep tem por conta própria, no macOS e no Windows, sem ferramenta externa. O
núcleo mora em `crates/keep-ia` (Rust) e é o mesmo para os dois apps:

- o app do macOS (Swift) chama o `keep` que vai dentro do app (`Contents/Resources/keep ia …` e
  `keep worktrees …`), que responde em JSON;
- o app do Windows (Tauri) chama as mesmas funções direto, pelo backend.

Os arquivos em disco seguem o formato que o kit-mac do autor já usava (`~/.claude/contas`,
`~/.codex-contas`), de modo que quem tinha o kit continua com as mesmas contas, a mesma ordem e
os mesmos logins; quem não tem começa do zero.

## Contas

### Claude

Uma conta do Claude é um **armazém de credenciais** do Claude Code:

| Armazém | Onde o Claude Code guarda o login | Como uma aba usa |
|---|---|---|
| global | macOS: item do Chaveiro `Claude Code-credentials` (conta `$USER`); demais: `~/.claude/.credentials.json` | sem variável |
| fixa | pasta `~/.claude/contas/fixas/<id>`; macOS: item `Claude Code-credentials-<sha256(pasta NFC)[:8]>`; demais: `<pasta>/.credentials.json` | `CLAUDE_SECURESTORAGE_CONFIG_DIR=<pasta>` |

- A variável `CLAUDE_SECURESTORAGE_CONFIG_DIR` muda **só** onde fica o login (lido no binário
  2.1.289, funções `Nb()`/`x$()`): histórico, memória e configurações continuam em `~/.claude`.
- Cada armazém tem o seu próprio login (aprovação no navegador). O Keep **nunca copia** token de um
  armazém para outro: o refresh token gira a cada renovação, e duas cópias da mesma família
  terminam com uma delas morta e a aba em "Not logged in".
- O Keep **renova** o login de cada armazém que cuida, no próprio armazém, antes de ele vencer e
  também na conta parada (ver "Renovação"): é o que deixa uma aba passar para qualquer conta sem
  login novo. O gerente externo (kit) renova o global; o Keep então renova só as pastas fixas.
- Dono de um armazém: `GET https://api.anthropic.com/api/oauth/profile` com o token dele
  (`User-Agent: claude-cli/<versão> (external, cli)`), em cache pela impressão do token. Nunca o
  `~/.claude.json`, que o CLI regrava com a conta do próprio token de qualquer aba.
- Metadados de uma pasta fixa criada pelo Keep: `<pasta>/keep.json`
  `{"versao":1,"apelido","email","uuid","criadaEm"}`. Pastas criadas pelo kit (`fixas/<uuid8>`)
  valem do mesmo jeito.
- Cofre do kit (`~/.claude/contas/<apelido>.json`): lido, nunca escrito — dá o apelido de cada
  dono e um token para medir o consumo de conta que não tenha armazém legível.
- Contas com o mesmo dono (global e fixa da mesma pessoa, ou dois apelidos do cofre) são uma conta só.

### GPT (Codex)

- `gpt:principal` = `~/.codex` (o `auth.json` do Codex).
- `gpt:<apelido>` = `~/.codex-contas/<apelido>`, usada com `CODEX_HOME=<pasta>`. Histórico,
  configuração e ganchos (`hooks.json`) do principal são ligados para dentro dela (link simbólico; junção
  no Windows).
- Dono: `chatgpt_account_id`/e-mail das declarações do `id_token`.

### Chaves e ordem

- Chave de conta: `claude:<apelido>`, `gpt:<apelido>`. Especial: `claude:ordem` = seguir a ordem.
- Ordem de prioridade: `~/.claude/contas/.ordem`, uma chave por linha (`#` e linhas vazias
  ignoradas). Contas que não estão no arquivo entram no fim (Claude: a global primeiro, depois por
  apelido; GPT: principal, depois por apelido; Claude antes de GPT).
- Disponível = sem aviso no login, sem `limitReached` e janelas 5h e 7d abaixo de 100% (sem
  medição = disponível).

### Gerente externo

Se o kit do autor estiver instalado (`~/.claude/contas/.ativa` e `~/.local/bin/claude-melhor-conta`),
quem troca a conta do armazém global é ele: no Keep, `claude:ordem` passa a ser "o global", uma aba
presa numa conta roda sempre na pasta própria dela (nunca no global, que muda de dono), e
`sincronizar` não faz nada (o kit tem a própria). Sem o kit, o Keep faz tudo.

## Renovação

Nenhuma conta cai, nem a parada: o que a Clínica faz com as contas dela (`renovarTokenSePossivel` +
`ia:healthcheck`) e o kit faz com o global (`claude-conectado`), para todo armazém do Keep.
`keep ia renovar [--agora] --json`; o app chama a cada 60 s, e logo ao abrir.

- Armazéns: as pastas fixas, sempre; o global só sem gerente externo. Nunca o cofre do kit, nunca
  cópia de um armazém para outro: o login renovado volta para o mesmo lugar de onde saiu.
- Quando: o acesso vence em menos de 60 min (o Claude Code só tenta nos 5 min finais, quando várias
  abas cruzam o limiar juntas e uma derruba a outra); ou o refresh não gira há mais de 7 dias (a
  conta parada: o refresh vive umas quatro semanas e morreria em silêncio). O nascimento de um
  refresh é o do acesso que veio com ele (`expiresAt` − 8 h). Acesso já vencido de conta em uso não
  é com o Keep: o Claude Code renova ao usar. `--agora` renova todos.
- Como: `POST https://platform.claude.com/v1/oauth/token` com `grant_type=refresh_token`, o
  `client_id` do Claude Code, os escopos do login e `User-Agent: claude-cli/<versão> (external,
  cli)` (sem ele, o Cloudflare responde 403/1010). Sob a trava de renovação do próprio Claude Code
  (`~/.claude/.oauth_refresh.lock`, um diretório, velha após 60 s, tocada a cada 2 s enquanto o
  Keep a segura): nenhuma renovação nossa corre junto com a de uma aba. A credencial é relida sob a
  trava; se o CLI já renovou, nada a fazer.
- Gravação: compare-and-swap sob a trava de escrita do CLI (`~/.claude/.storage-write.lock`, velha
  após 15 s): relê, e desiste se o refresh do armazém mudou enquanto o pedido estava no ar. Só a
  chave `claudeAiOauth` muda (acesso, refresh, `expiresAt`, `refreshTokenExpiresAt`, `scopes`); o
  resto do JSON fica (`mcpOAuth`…). No macOS, `security -i` com `add-generic-password -U … -X <hex>`
  pela entrada padrão (o token não aparece no `ps`), como o CLI grava; conferido lendo de volta.
- Recusa, no critério do CLI: `morto` (invalid_grant em 400/401) marca o armazém — a conta passa a
  dizer "recusado" no rodapé, nenhuma aba sobe nela (`precisa-login`) e ele não é tentado de novo
  até a credencial mudar (um login novo apaga a marca); `espera` (`account_on_hold`) tenta de novo
  em 1 h; o resto (sem rede, 429, 5xx, Cloudflare) tenta no próximo tique.
- Estado, sem token: `<estado>/renovar.json` (por armazém: impressão sha256[:12] do refresh,
  quando girou, a marca de morto e a espera); registro em `<estado>/renovar.log`. Uma rodada por
  vez (`<estado>/renovar.trava`).
- Resposta: `{"estado": "feito"|"em-andamento", "renovados": [{"conta", "armazem"}], "falhas":
  [{"conta", "armazem", "motivo"}]}`. `armazem` é `global` ou `fixa`; `conta`, a chave dela.

## Consumo

- Claude: `GET https://api.anthropic.com/api/oauth/usage` (`anthropic-beta: oauth-2025-04-20`).
- GPT: `GET https://chatgpt.com/backend-api/wham/usage` (`ChatGPT-Account-Id`).
- Uma medição por conta a cada 5 min; "Medir agora" força (no mínimo 15 s entre forçadas); 429
  respeita `Retry-After`. As leituras (sem token) ficam em `<estado>/uso.json`.

## Jev (crédito do OpenRouter)

O Jev (o modelo que decide no lugar da IA, pela skill `jev`: modelo e esforço de cada conversa e as
perguntas de decisão, no Claude Code e no Codex) gasta crédito do OpenRouter a cada decisão. O rodapé
"Consumo de IA" mostra quanto resta, ao lado das contas: `keep ia jev [--agora] [--cache]`.

- Chave: no macOS, o item `openrouter-api-key` do Chaveiro (`security find-generic-password -w`, o mesmo
  que a skill usa); no Windows, a credencial genérica `openrouter-api-key` do Gerenciador de Credenciais;
  nos outros, `<estado>/openrouter.key` (só do dono). Fora do macOS, `OPENROUTER_API_KEY` vale antes.
  Sem chave, `jev: null` e o rodapé não mostra nada (a leitura guardada some junto). A chave nunca vai
  para o cache.
- Mede: `GET https://openrouter.ai/api/v1/credits` (o que a conta comprou e gastou) e `/api/v1/key` (teto e
  gasto da chave do Jev). O que o Jev ainda pode gastar é o menor dos dois saldos.
- Dois contadores no rodapé: "conta" (o que resta do crédito; a barra é o gasto) e, para a chave, "chave"
  (o que resta do teto dela) ou, sem teto, "gasto" (o que ela gastou; a barra é essa parte do crédito
  todo). Dobrado, uma linha com o que o Jev ainda pode gastar.
- Conectar: o "+" do rodapé oferece "Conectar o Jev ao OpenRouter…" ("Reconectar…" quando já há chave):
  `keep ia entrar openrouter --ws=<W>` abre uma aba com `keep ia login openrouter`, o login do OpenRouter
  no navegador (OAuth PKCE: `https://openrouter.ai/auth` com volta em `http://localhost:<porta>/callback`,
  `POST /api/v1/auth/keys` troca o código pela chave). A chave "Jev (Keep)" é conferida (`/api/v1/key`),
  guardada no lugar acima e lida de volta; o saldo é medido na hora (o rodapé vê o `jev.json` mudar) e a
  aba se fecha. Sem limite de crédito na autorização, a chave usa todo o crédito da conta.
- Mesma cadência do consumo das contas (5 min; "Medir agora" a cada 15 s; 429 respeita `Retry-After`).
  Leitura guardada em `<estado>/jev.json`.
- Resposta: `{"jev": {"credit": {"total", "used", "keyLimit", "keyUsed", "usedToday", "usedThisMonth"},
  "measuredAt", "problem"}}`, em dólares.
- Teste: `KEEP_IA_OPENROUTER_URL` e `KEEP_IA_OPENROUTER_AUTH_URL` (só `http://127.0.0.1`),
  `KEEP_IA_SECURITY` e `KEEP_IA_TESTE_CHAVEIRO` (uma pasta no lugar do cofre, em todo sistema),
  `KEEP_IA_NAVEGADOR` (o programa que recebe o endereço do login) e `KEEP_IA_LOGIN_PRAZO` (segundos). O
  app do macOS aponta os endereços para uma porta sem ouvinte quando lê uma casa falsa.

## IA de cada aba

- O daemon diz o processo da frente de cada aba (`List3`): vínculo `exato`. Daemon antigo, sem
  `List3`: os shells filhos do keepd casados com as abas pelo título da conversa do Claude, pelo
  tamanho do terminal, pelo programa e pela pasta, só onde o par é único: vínculo `provavel`. O
  `KEEP_WORKSPACE` do ambiente do shell desempata (primeiro com ele, depois sem: uma aba movida de
  workspace leva no shell o nome do antigo). O título igual ao de uma conversa do shell vale mais que o
  programa que o daemon antigo diz: ele diz `zsh` de uma aba em que o Claude roda com um filho na
  frente (um servidor MCP, um php), e nesse caso a IA da aba é o Claude ou Codex que segura o
  terminal dentro do shell. Digitar
  numa aba vai sempre pelo daemon (a aba certa); o vínculo só diz de que processo ler a conta e a
  conversa, e o id da conversa vem, de preferência, da própria tela ao sair do programa.
- Conta da aba = ambiente do processo (`KEEP_IA_ESCOLHA`, ou o `CLAUDE_KIT_CONTA` do kit;
  `CLAUDE_SECURESTORAGE_CONFIG_DIR`; `CODEX_HOME`). O Keep põe `KEEP_IA_ESCOLHA=<chave>` na linha que
  sobe a IA, para lembrar a escolha ("seguir a ordem" ou uma conta) em qualquer conta e motor. Sem
  marca: Claude no global = seguir a ordem; numa pasta própria = presa nessa conta; Codex = preso na
  conta dele.
- Conversa da aba: Claude por `~/.claude/sessions/<pid>.json` (conferido pelo `procStart`, que diz
  também se ele está `busy`/`waiting`); Codex pela linha de comando (`resume <id>`) e pelo daemon dele
  (`app-server-control.sock`, só leitura e `turn/interrupt`; no Windows, só a tela).
- Os parâmetros com que a pessoa abriu a IA (modelo, esforço, permissões…) seguem para a próxima vez
  que ela subir na aba; a conversa, o texto inicial e os modos de rodar (`--print`, `--bg`…) não.

### Trocar

`trocar` (aba, para): confere a tela (trabalhando, diálogo, trabalho em segundo plano = `ocupada`, a
não ser com `--interromper`), sai do programa (`/exit` ou `/quit`), espera o shell, e digita a linha
do shell da aba (zsh/bash/fish, PowerShell, cmd) com o ambiente da conta e a retomada
(`claude --resume <id>` / `codex resume <id>`). Se a pasta da aba sumiu (uma worktree apagada), a
linha entra antes na pasta em que a conversa do Claude começou (ou na casa): a IA não sobe numa pasta
que não existe. Mudar de motor abre conversa nova no outro, com o
contexto visível da anterior num arquivo privado. Conta fixa sem login = `precisa-login`: a conversa
não sai, abre-se a aba do login, e a troca acontece sozinha quando ele termina.

### Entrar em outra conta

Aba nova rodando `keep ia login claude|gpt`: cria o armazém (pasta fixa nova, ou `CODEX_HOME` novo),
roda o login do Claude Code/Codex nele e, quando o login aparece, grava o `keep.json`, põe a conta no
fim da ordem e diz que terminou. A aba existe só para o login: quando ele dá certo, ela se fecha
sozinha 4 s depois (`--fechar-aba=<ws>:<n>`, que o Keep acrescenta ao abri-la); se falha, fica aberta
com o motivo.

### Seguir a ordem

"Seguir a ordem de prioridade" no menu manda sempre `claude:ordem`; o núcleo escolhe a primeira conta
do Claude disponível da fila em que uma aba pode rodar, e a aba leva a marca de que segue a ordem. O
GPT nunca entra sozinho: só quando a pessoa o escolhe no menu da aba. Sem nenhuma conta do Claude
disponível, a aba fica onde está e aparece como pendente (`todas-no-limite`). `sincronizar` (o app chama a cada 30 s): a aba que segue a ordem e está numa conta que não pode
trabalhar (no limite, login recusado, parada na tela de limite) vai para a primeira do Claude disponível da
fila, quando está livre; a que está numa conta abaixo dela sobe para ela depois de 2 min
sem uso. No máximo 3 abas por rodada; uma aba que recusou espera 30 s (5 min se for trabalho em
segundo plano). Uma janela cheia cujo recomeço já passou não conta.

## Lixeira de worktrees

Ao fechar uma aba, as worktrees git que a conversa dela criou vão para a Lixeira (Lixeira do Windows),
com os commits guardados em `refs/keep-lixeira/<id>`. Dono de uma worktree = a única sessão (Claude
ou Codex) que tinha, no nascimento dela, uma chamada executora em andamento que cita o nome dela.
Nunca: a principal, travada, em uso por outra aba viva, dono duvidoso. O app pergunta antes, lista no
diálogo e move (macOS: `FileManager.trashItem`, que grava o "Colocar de volta").

## Linha de comando (JSON, sempre `"versao": 1`; saída 0 = ok, 1 = recusado, 2 = uso errado)

```
keep ia contas --json
keep ia uso [--agora] [--conta=<chave>] --json
keep ia ordem [mover <chave> cima|baixo] --json
keep ia abas --json
keep ia trocar --ws=<W> --aba=<N> --para=<chave> [--interromper] --json
keep ia entrar claude|gpt --ws=<W> --json
keep ia login claude|gpt [--apelido=<A>] [--depois=<W>:<N>:<chave>]     (interativo, roda na aba)
keep ia sincronizar --json
keep ia renovar [--agora] --json
keep worktrees listar [--prazo SEG] <workspace>:<root>[,<painel>…]…
keep worktrees preparar <caminho>
keep worktrees concluir <caminho> <destino>
keep worktrees indexar [--json]
```

### Respostas

Toda resposta é um objeto JSON numa linha, com `"versao": 1` e `"ok"`. Recusa: `"ok": false`,
`"motivo"` e `"detalhe"` (português, para mostrar a quem usa). Datas em segundos unix (número).

- `contas` → `{"contas": [Conta], "ordem": [chave], "gerenteExterno": bool}`. Conta (nomes do
  `AIAccountSummary` do app do macOS): `engine` (`claude`|`codex`), `key` (identidade), `alias`,
  `aliases`, `email`, `plan`, `isActive`, `isPreferred`, `warning`, `orderKey`, `hasToken`, `roda`
  (há um armazém em que uma aba pode rodar nela), `armazens`.
- `uso` → `{"linhas": [Linha], "ordem": [chave], "gerenteExterno": bool, "medidoEm": s}`. Linha
  (`AccountUsage`): `account` (Conta), `reading` (`{"windows": [{"label", "title", "percent",
  "resetsAt"}], "limitReached"}` ou `null`), `measuredAt`, `problem`. `--cache` responde sem rede;
  `--agora` é o "Medir agora"; `--conta=<chave>` mede só essa.
- `ordem [mover …]` → `{"ordem": [chave]}`.
- `abas` → `{"keepd": {"pid", "inicioMs"}, "ia": [{"workspace", "aba", "agente"
  (`claude`|`codex`), "conta" (a escolha: `claude:ordem` ou a chave da conta), "atual" (a chave da
  conta em que ela roda agora, ou `null`), "vinculo" (`exato`), "pid", "conversa" (id ou `null`)}]}`.
  `keepd.inicioMs` é o `started_ms` do `List3`: o app ignora a resposta de outro daemon.
- `trocar` → `{"feito": texto}`. Recusas: `ocupada` (o app pergunta "Interromper e trocar agora" e
  repete com `--interromper`), `precisa-login` (com `ws_login` e `aba_login`: a aba do login, aberta
  pelo Keep; a troca acontece sozinha quando ele termina), `em-uso`, `segundo-plano`,
  `outro-programa`, `tela-inesperada`, `sem-conta`, `contexto-indisponivel`, `inicio-falhou`, `erro`.
- `entrar` → `{"ws", "aba"}`: a aba nova, rodando `keep ia login …`.
- `sincronizar` → `{"estado": "feito"|"gerente-externo"|"em-andamento"|"sem-keepd", "alvo": chave|null,
  "alteradas": [{"ws", "aba", "feito"}], "pendentes": [{"ws", "aba", "motivo", "detalhe"?}]}`.
- `renovar` → `{"estado": "feito"|"em-andamento", "renovados": [{"conta", "armazem"}], "falhas":
  [{"conta", "armazem", "motivo"}]}` (ver "Renovação").

### Quem chama o quê

| Quando | Comando |
|---|---|
| a cada 60 s, com o rodapé aberto (o período de 5 min é do núcleo) | `keep ia uso --json` |
| "Medir agora" | `keep ia uso --agora --json` |
| um arquivo de login mudou (vigia barato, sem rede) | `keep ia uso --cache --json` |
| a cada 5 s com a janela à frente, e ao abrir o menu de uma aba | `keep ia abas --json` |
| a cada 30 s | `keep ia sincronizar --json` |
| a cada 60 s, e ao abrir (mesmo sem o daemon) | `keep ia renovar --json` |
| a cada 2 min | `keep worktrees indexar --json` |
| setas, "+", menu da aba, fechar aba | `ordem mover`, `entrar`, `trocar`, `worktrees listar/preparar/concluir` |

O app passa `KEEP_SOCKET` (o daemon dele). Um app de teste que lê uma casa falsa passa
`KEEP_IA_HOME` e `KEEP_IA_ESTADO` apontando para ela: o núcleo nunca toca na casa real nesse caso.

Variáveis para teste: `KEEP_IA_HOME` (casa falsa), `KEEP_IA_ESTADO` (estado), `KEEP_IA_SECURITY`
(o `security` do macOS), `KEEP_IA_CLAUDE_URL`/`KEEP_IA_CODEX_URL`/`KEEP_IA_PERFIL_URL`/`KEEP_IA_TOKEN_URL` (só
`http://127.0.0.1`), `KEEP_IA_CLAUDE_BIN`, `KEEP_IA_CODEX_BIN`, `KEEP_IA_KEEP_BIN` (o `keep` que uma aba nova roda), `KEEP_SOCKET`.
Teste de ponta a ponta: `trocar::ponta_a_ponta`, com um daemon do próprio teste e a IA falsa de
`examples/ia_falsa.rs` (`cargo build -p keep-ia --examples`).
