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

| Armazém | Onde o Claude Code guarda o login | Quem usa |
|---|---|---|
| global | macOS: item do Chaveiro `Claude Code-credentials` (conta `$USER`); demais: `~/.claude/.credentials.json` | sem variável |
| fixa | pasta `~/.claude/contas/fixas/<id>`; macOS: item `Claude Code-credentials-<sha256(pasta NFC)[:8]>`; demais: `<pasta>/.credentials.json` | o dono da conta: só a renovação do Keep |
| aba | pasta `~/.claude/contas/abas/<shell>-<nascimento do shell>`, guardada como a fixa | o Claude de uma aba: `CLAUDE_SECURESTORAGE_CONFIG_DIR=<pasta>` |

- A variável `CLAUDE_SECURESTORAGE_CONFIG_DIR` muda **só** onde fica o login (lido no binário
  2.1.296, funções `Lw()`/`j$()`): histórico, memória e configurações continuam em `~/.claude`. As
  travas do CLI para um armazém moram na pasta dele (`<pasta>/.oauth_refresh.lock` ao renovar,
  `<pasta>/.storage-write.lock` ao gravar; `~/.claude` para o global).
- Um dono por conta: o armazém que guarda a credencial inteira dela (a pasta fixa; sem o kit, o
  global quando é dela). Cada dono tem o seu próprio login (aprovação no navegador). O Keep **nunca
  copia refresh token** de um armazém para outro: ele gira a cada renovação, e duas cópias da mesma
  família terminam com uma delas morta e a aba em "Not logged in". O que vai para os armazéns das
  abas é só o token de ACESSO (ver "Troca a quente").
- O Keep **renova** o login de cada armazém que cuida, no próprio armazém, antes de ele vencer e
  também na conta parada (ver "Renovação"): é o que deixa uma aba passar para qualquer conta sem
  login novo. O gerente externo (kit) renova o global; o Keep então renova só as pastas fixas.
- Dono de um armazém: `GET https://api.anthropic.com/api/oauth/profile` com o token dele
  (`User-Agent: claude-cli/<versão> (external, cli)`), em cache pela impressão do token. Nunca o
  `~/.claude.json`, que o CLI regrava com a conta do próprio token de qualquer aba.
- Metadados de uma pasta fixa criada pelo Keep: `<pasta>/keep.json`
  `{"versao":1,"apelido","email","uuid","criadaEm"}`. Pastas criadas pelo kit (`fixas/<uuid8>`)
  valem do mesmo jeito. `"aba": true` marca uma pasta fixa que virou o armazém de uma aba: ela deixa
  de ser conta e de ser renovada.
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

- Para a pessoa, uma conta é a posição dela na lista do rodapé "Consumo de IA": `IA-n`, contando de
  1, Claude e GPT juntos, na ordem efetiva (pedido de 13/10/2026: nada de "principal" ou "reserva").
  Todo texto do núcleo para quem usa (`feito`, `detalhe`, o que a aba do login imprime) diz assim; as
  chaves abaixo ficam só por dentro e no JSON.

- Chave de conta: `claude:<apelido>`, `gpt:<apelido>`. Especial: `claude:ordem` = seguir a ordem.
- Ordem de prioridade: `~/.claude/contas/.ordem`, uma chave por linha (`#` e linhas vazias
  ignoradas). Contas que não estão no arquivo entram no fim (Claude: a global primeiro, depois por
  apelido; GPT: principal, depois por apelido; Claude antes de GPT).
- Disponível = sem aviso no login, sem `limitReached` e janelas 5h e 7d abaixo de 100% (sem
  medição = disponível).

### Gerente externo

Se o kit do autor estiver instalado (`~/.claude/contas/.ativa` e `~/.local/bin/claude-melhor-conta`),
quem troca a conta do armazém global é ele: no Keep, `claude:ordem` passa a ser a conta que ele pôs
no global (`.ativa`) — as abas de armazém próprio que seguem a ordem vão para ela na renovação
seguinte, a quente —, o dono de uma conta é a pasta fixa dela (o global só por último: ele muda de
dono), o global não é renovado pelo Keep, e `sincronizar` não faz nada (o kit tem a própria). Sem o
kit, o Keep faz tudo.

## Renovação

Nenhuma conta cai, nem a parada: o que a Clínica faz com as contas dela (`renovarTokenSePossivel` +
`ia:healthcheck`) e o kit faz com o global (`claude-conectado`), para todo armazém do Keep.
`keep ia renovar [--agora] --json`; o app chama a cada 60 s, e logo ao abrir.

- Armazéns: as pastas fixas (menos as que viraram armazém de aba), sempre; o global só sem gerente
  externo. Nunca o cofre do kit, nunca cópia de um armazém para outro: o login renovado volta para o
  mesmo lugar de onde saiu.
- Quando: o acesso vence em menos de 60 min (o Claude Code só tenta nos 5 min finais, quando várias
  abas cruzam o limiar juntas e uma derruba a outra); o acesso já venceu (nenhuma aba renova um
  dono: cada uma tem só o acesso); ou o refresh não gira há mais de 7 dias (a conta parada: o
  refresh vive umas quatro semanas e morreria em silêncio). O nascimento de um refresh é o do acesso
  que veio com ele (`expiresAt` − 8 h). `--agora` renova todos.
- Depois de renovar, a rodada espalha o acesso de cada dono pelos armazéns das abas da conta dele e
  apaga os das abas que acabaram (ver "Troca a quente"). O dono do acesso novo é o do antigo: ele
  fica sabido sem perguntar à rede.
- Como: `POST https://platform.claude.com/v1/oauth/token` com `grant_type=refresh_token`, o
  `client_id` do Claude Code, os escopos do login e `User-Agent: claude-cli/<versão> (external,
  cli)` (sem ele, o Cloudflare responde 403/1010). Sob a trava de renovação do próprio Claude Code
  para aquele armazém (`<pasta>/.oauth_refresh.lock`, um diretório, velha após 60 s, tocada a cada
  2 s enquanto o Keep a segura): nenhuma renovação nossa corre junto com a de um Claude que use o
  mesmo armazém. A credencial é relida sob a trava; se o CLI já renovou, nada a fazer.
- Gravação: compare-and-swap sob a trava de escrita do CLI (`<pasta>/.storage-write.lock`, velha
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
  [{"conta", "armazem", "motivo"}], "abas": {"trocadas": [{"chave", "conta"}], "atualizadas": n,
  "apagadas": [chave], "falhas": [{"chave", "motivo"}]}}`. `armazem` é `global` ou `fixa`; `conta`,
  a chave dela. Em `abas`, um problema aparece uma vez, quando surge (fica no registro do armazém até
  passar).

## Troca a quente

Pedido de 12/10/2026: "pra trocar entre contas do claude não é pra colocar exit, é só trocar". Trocar
a conta do Claude de uma aba é regravar o armazém de login dele; o mesmo processo, na mesma conversa,
passa a falar pela conta nova. A conexão persistente do Chrome acompanha por uma reconexão local,
assim que o cache de login venceu e a aba ficou livre.

- Cada Claude que o Keep abre roda no armazém da aba (`CLAUDE_SECURESTORAGE_CONFIG_DIR=
  ~/.claude/contas/abas/<shell>-<nascimento>`), que tem o `claudeAiOauth` da conta da aba com
  `refreshToken: null` (e sem `refreshTokenExpiresAt`): só o acesso. O Claude Code relê o armazém a
  cada uso (no macOS, o item do Chaveiro com cache de 30 s, `j0t`; no Windows, o `.credentials.json`
  da pasta, relido quando muda) e, sem refresh, nunca tenta renovar nem grava o login. Provado em
  12/10/2026 com o Claude Code 2.1.296: a conta mudou em até 30 s, sem /exit, e um acesso inválido no
  armazém, regravado com um válido, deixou a mesma sessão voltar.
- A ponte do Chrome já conectada conserva a autenticação anterior até reconectar. Toda mudança
  de dono, manual ou pela ordem, marca `chrome_pendente_desde` no armazém da aba. A rodada de
  `renovar` aguarda pelo menos 32 s e envia `/mcp reconnect claude-in-chrome` quando a mesma aba
  está livre, sem diálogo nem rascunho, com vínculo exato ao processo. É um comando local:
  não inicia uma resposta do modelo, não sai do Claude e não muda a conversa ou o login global.
  Renovar o token da mesma conta não marca reconexão. Se o Claude já reiniciou depois da troca,
  a pendência é descartada; uma nova troca não é apagada pela conclusão da anterior.
  `keep ia reconectar-chrome --json` processa as mesmas pendências para diagnóstico.
- `keep ia preparar --ws=<W> --shell=<pid> [--escolha=<chave>]` cria (ou atualiza) o armazém da aba
  do shell e devolve a pasta. O shell é o dado ou o ancestral dele cujo pai é o keepd (um script na
  aba acha a aba). Sem escolha, fica a que a aba já tinha, senão seguir a ordem. Quem chama: a função
  `claude()` do shell do autor (numa aba do Keep, sem a variável já posta) e a troca que sobe o
  Claude numa aba (`trocar`, linha com `CLAUDE_SECURESTORAGE_CONFIG_DIR=<pasta>`).
- Estado: `~/.claude/contas/abas/estado.json` (`{"versao":1,"abas":{<chave>:{"pasta","conta","dono",
  "escolha","ws","aba","shell","nascimento","legada","em","falha"}}}`), sob a trava
  `estado.trava`. A conta e a escolha de uma aba de armazém próprio vêm daqui (o ambiente do
  processo diz só as de quando ele subiu).
- De onde vem o acesso: o dono da conta, com o acesso em dia (sem o kit, o global quando é dela; as
  pastas fixas dela; com o kit, o global por último). Dono com o acesso vencido: uma rodada da
  renovação antes. Seguir a ordem = com o kit, a conta que ele pôs no login global (`.ativa`); sem
  ele, a primeira do Claude disponível da fila (pela última medição), ou a primeira que roda.
- A gravação num armazém de aba muda só a chave `claudeAiOauth` (os `mcpOAuth` que o Claude da aba
  guardou ficam), sob as duas travas do CLI daquela pasta.
- Renovação (`renovar`, a cada minuto): cada armazém de aba recebe o acesso atual do dono da conta
  dele; com o kit, quem segue a ordem passa para a conta da `.ativa`. Sem o kit, a conta de quem segue
  a ordem só muda pela sincronização (as regras dela). O armazém de uma aba cujo shell acabou (pid e
  nascimento) e em que nenhum processo roda mais (uma conversa em segundo plano segura o dela) é
  apagado: o item e a pasta; pastas soltas em `abas/` também.
- Login feito dentro de uma aba (`/login`): cai no armazém dela, com refresh. Ele é a escolha da aba;
  a credencial inteira MUDA para uma pasta fixa nova quando a conta não tem outro dono (conta nova
  entra no fim da ordem), e o refresh a mais é descartado quando tem. O armazém fica só com o acesso.
- Abas de antes: um Claude numa pasta fixa só dele (nenhum outro processo raiz com a mesma variável)
  tem a pasta convertida na troca: marcada `"aba": true`, registrada como `fixa-<nome>` (legada), e
  o login inteiro sai dela — muda para uma pasta fixa nova se ela era a única dona da conta (com o
  kit, o global não conta como outra dona: ele muda de conta quando o kit quer). Pasta
  fixa usada por mais de um Claude: `pasta-compartilhada`. Claude no login global (sem variável):
  `claude-global` — a conta dele muda com a ordem global, junto com as outras abas no global; o
  próximo Claude aberto na aba já vem no armazém dela.
- Quem desligar o app por mais de umas 8 h deixa as abas sem acesso válido: a primeira rodada da
  renovação ao abrir renova os donos e espalha, e cada aba volta no pedido seguinte.

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
  (o que resta do teto dela) ou, sem teto, "gasto" (o que a conta toda já gastou, de todas as chaves, e
  não só a do Jev; a dica diz quanto foi da chave do Jev). Dobrado, uma linha com o que o Jev ainda pode
  gastar.
- Conectar: o "+" do rodapé oferece "Conectar o Jev ao OpenRouter…" ("Reconectar…" quando já há chave):
  `keep ia entrar openrouter --ws=<W>` abre uma aba com `keep ia login openrouter`, o login do OpenRouter
  no navegador (OAuth PKCE: `https://openrouter.ai/auth` com volta em `http://localhost:<porta>/callback`,
  `POST /api/v1/auth/keys` troca o código pela chave). A chave "Jev (Keep)" é conferida (`/api/v1/key`),
  guardada no lugar acima e lida de volta; o saldo é medido na hora (o rodapé vê o `jev.json` mudar) e a
  aba se fecha. Sem limite de crédito na autorização, a chave usa todo o crédito da conta.
- Uma leitura por minuto (duas consultas de metadados, que o OpenRouter não cobra); "Medir agora" a cada
  15 s; 429 respeita `Retry-After`. Leitura guardada em `<estado>/jev.json`.
- Tempo real: o OpenRouter leva uns dois minutos para contar uma decisão (2 min 20 s medidos em
  05/10/2026). A skill `jev` anota cada chamada paga em `~/.claude/jev/chamadas.jsonl`, uma linha JSON
  por chamada: `{"quando": <segundos desde 1970>, "custo_usd": <dólares, o "usage.cost" da resposta>,
  "chave": <12 primeiros dígitos hexadecimais do SHA-256 da chave, sem o fim de linha>}`. O núcleo soma
  ao saldo as chamadas da chave dele que a leitura ainda não contou (`pending`) e, a cada leitura nova,
  casa o que a chave gastou desde a anterior com as chamadas mais antigas do livro (1% de folga para o
  arredondamento; a sobra que não cobre uma chamada inteira fica para a leitura seguinte). Chamada não
  contada em 10 min sai da soma; chave nova começa o livro do zero. Os dois apps vigiam o livro no
  olhar de 2 s e pedem `--cache`: uma decisão aparece no rodapé em uns 2 s, sem ir à rede.
- Os valores do Jev têm duas casas decimais: o número na tela muda quando o gasto acumulado das decisões
  completa um centavo (uma decisão custa de US$ 0,00002 a US$ 0,0002).
- Resposta: `{"jev": {"credit": {"total", "used", "keyLimit", "keyUsed", "usedToday", "usedThisMonth"},
  "measuredAt", "problem", "pending"}}`, em dólares; `credit` já inclui `pending`, que só aparece quando há
  chamada à espera do OpenRouter.
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
  conta dele. Claude no armazém de uma aba: a escolha e a conta são as do estado dele (mudam com o
  processo vivo).
- Conversa da aba: Claude por `~/.claude/sessions/<pid>.json` (conferido pelo `procStart`, que diz
  também se ele está `busy`/`waiting`); Codex pela linha de comando (`resume <id>`) e pelo daemon dele
  (`app-server-control.sock`, só leitura e `turn/interrupt`; no Windows, só a tela).
- Os parâmetros com que a pessoa abriu a IA (modelo, esforço, permissões…) seguem para a próxima vez
  que ela subir na aba; a conversa, o texto inicial e os modos de rodar (`--print`, `--bg`…) não.

### Trocar

`trocar` (aba, para): de uma conta do Claude para outra com o Claude aberto, a troca a quente (acima):
nada sai, nada é digitado, vale com a aba trabalhando. Nos outros casos, confere a tela (trabalhando,
diálogo, trabalho em segundo plano = `ocupada`, a não ser com `--interromper`), sai do programa
(`/exit` ou `/quit`), espera o shell, e digita a linha do shell da aba (zsh/bash/fish, PowerShell, cmd)
com o ambiente da conta (para o Claude, o armazém da aba) e a retomada
(`claude --resume <id>` / `codex resume <id>`). Se a pasta da aba sumiu (uma worktree apagada), a
linha entra antes na pasta em que a conversa do Claude começou (ou na casa): a IA não sobe numa pasta
que não existe. Mudar de motor abre conversa nova no outro, com o
contexto visível da anterior num arquivo privado. Conta fixa sem login = `precisa-login`: a conversa
não sai, abre-se a aba do login, e a troca acontece sozinha quando ele termina.

O resumo chega pronto na primeira mensagem: `keep ia iniciar-com-contexto` lê o cartão local e passa
seu conteúdo diretamente como argumento da IA, sem inserir mensagens no comando do shell. O arquivo
continua disponível para consulta; a IA não precisa gastar uma chamada para abri-lo. Comandos locais
como `/effort`, análises internas e atualizações intermediárias do Codex ficam fora do resumo.
A orientação é continuar somente trabalho autorizado pendente; uma tarefa concluída recebe uma
confirmação curta, sem repetir testes, consultas ao Git ou pesquisas por causa da troca.
No macOS, o nome exibido da aba fica preservado durante a troca e salvo quando ela é aceita, inclusive
quando depende de login. Um nome escolhido pela pessoa durante a troca tem prioridade.

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
fila — com o Claude aberto num armazém de aba, a quente e mesmo trabalhando; sem ele, quando está livre;
com o Claude aberto no login global, nunca (`claude-global`) —; a que está numa conta abaixo dela sobe
para ela depois de 2 min sem uso. Com o kit, a sincronização não faz nada, e a renovação leva as abas
de armazém próprio que seguem a ordem para a conta da `.ativa`. No máximo 3 abas por rodada; uma aba que recusou espera 30 s (5 min se for trabalho em
segundo plano). Uma janela cheia cujo recomeço já passou não conta.

## Lixeira de worktrees

Ao fechar uma aba, as worktrees git que a conversa dela criou vão para a Lixeira (Lixeira do Windows),
com os commits guardados em `refs/keep-lixeira/<id>` e a pasta inteira como o git a guardaria
(alterações rastreadas, apagados e arquivos novos não ignorados, num índice à parte, sem mexer na pasta
nem no índice dela) em `refs/keep-lixeira/<id>-sujo`. A Lixeira guarda a pasta por 3 dias; depois o
próprio Keep a apaga de vez (`keep worktrees purgar`, logo depois da varredura), e o que vale são os
`refs/keep-lixeira`. A purga só apaga o que o Keep mandou e ainda é ele: o destino exato no
`lixeira.log`, o número da pasta no volume (`ino`; no Windows, o índice do NTFS) e o nascimento dela,
gravados na hora da ida, e o `.git` de worktree (`gitdir:`). Item que voltou ("Colocar de volta"), item
da pessoa com o mesmo nome, pasta mexida, linha de antes desta versão ou fora da Lixeira: fica. Cada
purga é uma linha no `lixeira.log` (`"evento": "purga"`); nenhuma configuração do Finder é tocada.
Dono de uma worktree = a única sessão (Claude
ou Codex) que tinha, no nascimento dela, uma chamada executora em andamento que cita o nome dela.
Nunca: a principal, travada, em uso por outra aba viva, dono duvidoso. O app pergunta antes, lista no
diálogo e move (macOS: `FileManager.trashItem`, que grava o "Colocar de volta").

A aba que acaba sem o diálogo — o shell sai (`exit`, Ctrl-D), o workspace acaba por outro caminho, o
computador reinicia e ela não volta — também leva as worktrees dela: o `indexar` anota as conversas
cujas abas fecharam (`encerradas`, por 30 dias), e logo depois dele o app pergunta `keep worktrees
orfas` e move o que vier em `lixeira` pelos mesmos três passos (preparar, mover, concluir), sem
diálogo. Órfã = dona decidida (não provisória) que rodou numa aba do Keep e não tem mais aba (nem ela,
nem a conversa em que ela seguiu, `continuedInSessionId`), não roda em lugar nenhum (sessão do Claude
viva, thread aberta num Codex, o id na linha de comando de qualquer processo — o cartão de retomada) e
não é devida pela restauração das abas (`keep restauro`: enquanto ela não acaba neste keepd, toda
conversa dos retratos; depois, a de cada aba recriada que segue aberta). Na dúvida, fica: conversa que
nunca passou por uma aba (Terminal, editor, `claude -p` agendado), transcrito mexido há menos de 20 min,
keepd sem `List3` ou de pé há menos de 20 min, worktree principal, travada, com submódulos, já na
Lixeira, com processo dentro ou com outra conversa viva trabalhando nela. Só vai depois de órfã por 20
min seguidos (a carência recomeça se a conversa voltar). Registro cuja pasta já sumiu sai sozinho
(`esquecer`: só aquele registro, com a HEAD guardada; nunca `git worktree prune`; pasta em disco que
pode estar desmontado fica). Depois de renomear a pasta dos projetos (mesmo só na caixa), o `indexar`
refaz as chaves das decisões pelo caminho canônico, e nenhuma decisão se perde.

## Linha de comando (JSON, sempre `"versao": 1`; saída 0 = ok, 1 = recusado, 2 = uso errado)

```
keep ia contas --json
keep ia uso [--agora] [--conta=<chave>] --json
keep ia ordem [mover <chave> cima|baixo] --json
keep ia abas --json
keep ia trocar --ws=<W> --aba=<N> --para=<chave> [--interromper] --json
keep ia preparar --ws=<W> --shell=<pid> [--escolha=claude:ordem|claude:<apelido>] --json
keep ia entrar claude|gpt --ws=<W> --json
keep ia login claude|gpt [--apelido=<A>] [--depois=<W>:<N>:<chave>]     (interativo, roda na aba)
keep ia sincronizar --json
keep ia renovar [--agora] --json
keep worktrees listar [--prazo SEG] <workspace>:<root>[,<painel>…]…
keep worktrees preparar <caminho>
keep worktrees concluir <caminho> <destino>
keep worktrees indexar [--json]
keep worktrees orfas [--json] [--so-ler]
keep worktrees esquecer <caminho> [--nasceu NS]
keep worktrees purgar [--json]
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
  conta em que ela roda agora, ou `null`), "vinculo" (`exato`), "pid", "conversa" (id ou `null`),
  "provada" (só no Claude, e só quando se prova: a conta em que ele respondeu por último, pelos
  recomeços dos `rate_limits` da statusLine casados com os medidos no `uso`)}]}`. Logo depois de
  uma troca a quente, `provada` ainda é a conta antiga: o Claude relê o armazém a cada 30 s e só
  usa o token novo na resposta seguinte; a lateral do app diz "(IA·1→2)" até lá.
  `keepd.inicioMs` é o `started_ms` do `List3`: o app ignora a resposta de outro daemon.
- `conta-da-aba [--gancho]` (texto, não JSON): a conta do Claude desta aba ("IA·2 (e-mail)"), lida do
  armazém da aba (`CLAUDE_SECURESTORAGE_CONFIG_DIR`), não do `~/.claude.json` — esse é o login global e
  não acompanha a troca a quente. Com `--gancho` é o `UserPromptSubmit` do Claude Code
  (`~/.claude/settings.json`): devolve o aviso como `additionalContext` quando a conta da aba não é a
  global, ou quando mudou desde o último aviso na conversa (`<estado>/avisos-de-conta/<sessão>`).
- `trocar` → `{"feito": texto}` (a quente: "passou para a IA-2 sem sair do Claude"). Recusas:
  `ocupada` (o app pergunta "Interromper e trocar agora" e repete com `--interromper`),
  `precisa-login` (com `ws_login` e `aba_login`: a aba do login, aberta pelo Keep; a troca acontece
  sozinha quando ele termina), `claude-global`, `pasta-compartilhada`, `armazem-desconhecido`,
  `em-uso`, `segundo-plano`, `outro-programa`, `tela-inesperada`, `sem-conta`,
  `contexto-indisponivel`, `inicio-falhou`, `erro`.
- `preparar` → `{"chave", "pasta", "conta", "escolha"}`. Recusas: `uso`, `sem-conta`,
  `precisa-login`, `ocupada`, `erro` — quem chama sobe o Claude como antes, sem a variável.
- `entrar` → `{"ws", "aba"}`: a aba nova, rodando `keep ia login …`.
- `sincronizar` → `{"estado": "feito"|"gerente-externo"|"em-andamento"|"sem-keepd", "alvo": chave|null,
  "alteradas": [{"ws", "aba", "feito"}], "pendentes": [{"ws", "aba", "motivo", "detalhe"?}]}`.
- `renovar` → `{"estado": "feito"|"em-andamento", "renovados": [{"conta", "armazem"}], "falhas":
  [{"conta", "armazem", "motivo"}], "abas": {…}}` (ver "Renovação").
- `worktrees orfas` → `{"lixeira": [item do listar], "registros": [item: a pasta já sumiu], "aguardando":
  [{"caminho", "motivo"}], "mantidas": [{"caminho", "motivo"}], "avisos": [texto], "ocupado"?}`. Grava
  desde quando cada uma é órfã (`--so-ler`: não grava nada). `ocupado`: outra rodada em curso.
- `worktrees esquecer` → como o `concluir` (`ok`, `registro_removido`, `ref`, `motivo`), para um
  registro cuja pasta já não existe.
- `worktrees purgar` → `{"purgadas": [{"destino", "caminho", "id", "ref", "ref_sujo"}], "ignoradas":
  [{"destino", "motivo"}], "avisos": [texto], "ocupado"?}`.

### Quem chama o quê

| Quando | Comando |
|---|---|
| a cada 60 s, com o rodapé aberto (o período de 5 min é do núcleo) | `keep ia uso --json` |
| "Medir agora" | `keep ia uso --agora --json` |
| um arquivo de login mudou (vigia barato, sem rede) | `keep ia uso --cache --json` |
| a cada 5 s com a janela à frente, e ao abrir o menu de uma aba | `keep ia abas --json` |
| a cada 30 s | `keep ia sincronizar --json` |
| a cada 60 s, e ao abrir (mesmo sem o daemon) | `keep ia renovar --json` |
| a cada 2 min | `keep worktrees indexar --json` e, logo depois, `keep worktrees orfas --json` (com `preparar`/`concluir` ou `esquecer` para o que vier) e `keep worktrees purgar --json` |
| setas, "+", menu da aba, fechar aba | `ordem mover`, `entrar`, `trocar`, `worktrees listar/preparar/concluir` |

O app passa `KEEP_SOCKET` (o daemon dele). Um app de teste que lê uma casa falsa passa
`KEEP_IA_HOME` e `KEEP_IA_ESTADO` apontando para ela: o núcleo nunca toca na casa real nesse caso.

Variáveis para teste: `KEEP_IA_HOME` (casa falsa), `KEEP_IA_ESTADO` (estado), `KEEP_IA_SECURITY`
(o `security` do macOS), `KEEP_IA_TESTE_ARMAZEM=arquivo` (no macOS, os logins em arquivo, como no
Windows), `KEEP_IA_CLAUDE_URL`/`KEEP_IA_CODEX_URL`/`KEEP_IA_PERFIL_URL`/`KEEP_IA_TOKEN_URL` (só
`http://127.0.0.1`), `KEEP_IA_CLAUDE_BIN`, `KEEP_IA_CODEX_BIN`, `KEEP_IA_KEEP_BIN` (o `keep` que uma aba nova roda), `KEEP_SOCKET`.
Teste de ponta a ponta: `trocar::ponta_a_ponta`, com um daemon do próprio teste e a IA falsa de
`examples/ia_falsa.rs` (`cargo build -p keep-ia --examples`); o Claude falso relê o armazém a cada
pedido e responde `Feito: <pedido>, conta=<token>`, que é como a troca a quente aparece na tela. Os
armazéns das abas: `quente::testes`, no Chaveiro de mentira e no arquivo.
