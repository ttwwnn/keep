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
- O Keep **nunca renova** token: quem renova é o próprio Claude Code, a cada uso.
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
- `gpt:<apelido>` = `~/.codex-contas/<apelido>`, usada com `CODEX_HOME=<pasta>`. Histórico e
  configuração do principal são ligados para dentro dela (link simbólico; junção no Windows).
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
quem troca a conta do armazém global é ele: no Keep, `claude:ordem` passa a ser "o global", e a
sincronização automática do Keep não move abas de um Claude para outro (só de motor). Sem o kit, o
Keep faz tudo.

## Consumo

- Claude: `GET https://api.anthropic.com/api/oauth/usage` (`anthropic-beta: oauth-2025-04-20`).
- GPT: `GET https://chatgpt.com/backend-api/wham/usage` (`ChatGPT-Account-Id`).
- Uma medição por conta a cada 5 min; "Medir agora" força (no mínimo 15 s entre forçadas); 429
  respeita `Retry-After`. As leituras (sem token) ficam em `<estado>/uso.json`.

## IA de cada aba

- O daemon diz o processo da frente de cada aba (`List3`). Daemon antigo, sem `List3`: casamento por
  programa + pasta + tamanho, marcado `provavel` (só leitura; trocar exige `exato`).
- Conta da aba = ambiente do processo (`KEEP_IA_ESCOLHA`, `CLAUDE_SECURESTORAGE_CONFIG_DIR`,
  `CODEX_HOME`). O Keep põe `KEEP_IA_ESCOLHA=<chave>` na linha que sobe a IA, para lembrar a escolha
  ("seguir a ordem" ou uma conta) mesmo quando ela roda numa pasta fixa.
- Conversa da aba: Claude por `~/.claude/sessions/<pid>.json`; Codex pela linha de comando
  (`resume <id>`) e pelo daemon dele.

### Trocar

`trocar` (aba, para): confere a tela (trabalhando, diálogo, trabalho em segundo plano = `ocupada`, a
não ser com `--interromper`), sai do programa (`/exit` ou `/quit`), espera o shell, e digita a linha
do shell da aba (zsh/bash/fish, PowerShell, cmd) com o ambiente da conta e a retomada
(`claude --resume <id>` / `codex resume <id>`). Mudar de motor abre conversa nova no outro, com o
contexto visível da anterior num arquivo privado. Conta fixa sem login = `precisa-login`: a conversa
não sai, abre-se a aba do login, e a troca acontece sozinha quando ele termina.

### Entrar em outra conta

Aba nova rodando `keep ia login claude|gpt`: cria o armazém (pasta fixa nova, ou `CODEX_HOME` novo),
roda o login do Claude Code/Codex nele e, quando o login aparece, grava o `keep.json`, põe a conta no
fim da ordem e diz que terminou.

### Seguir a ordem

`sincronizar` (o app chama a cada 30 s): a aba que segue a ordem e está numa conta indisponível (ou
parada na tela de limite) vai para a primeira disponível da fila, quando está livre. No máximo 3 abas
por rodada; esperas guardadas por aba.

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
keep worktrees listar [--prazo SEG] <workspace>:<root>[,<painel>…]…
keep worktrees preparar <caminho>
keep worktrees concluir <caminho> <destino>
keep worktrees indexar [--json]
```

Variáveis para teste: `KEEP_IA_HOME` (casa falsa), `KEEP_IA_ESTADO` (estado), `KEEP_IA_SECURITY`
(o `security` do macOS), `KEEP_IA_CLAUDE_URL`/`KEEP_IA_CODEX_URL`/`KEEP_IA_PERFIL_URL` (só
`http://127.0.0.1`), `KEEP_IA_CLAUDE_BIN`, `KEEP_IA_CODEX_BIN`, `KEEP_SOCKET`.
