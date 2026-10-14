# Paridade do Keep no Windows com o Mac

Meta (13/10/2026): tudo o que existe no app do Mac existe no app do Windows,
com a mesma aparência. Este arquivo é a lista viva. Cada fatia marca o que fez;
o que não puder ser feito diz por quê.

Estados: `[x]` feito e testado, `[~]` feito sem poder testar no Windows de
verdade (só compilado no CI), `[ ]` por fazer, `[-]` não se aplica.

## Aparência (feito em 13/10/2026)

- [x] Textos x1,25, tintas, barra lateral, fileira de abas, zoom, bateria, RAM,
  rodapé, barra de status, cartões e menus.
- [x] Cursor em barra. Atalhos: barra lateral, workspace seguinte/anterior,
  selecionar tudo, limpar a tela, tela cheia, configurações.

## Fatia `palette` (seletor, paleta, busca)

- [ ] Ir para (Ctrl+Shift+P): recentes primeiro (a 1ª linha é a última aba de
  OUTRO workspace), cabeçalhos "terminals" e "open in", linha rica, teclas
  Ctrl+J/K/N/P, duplo clique, Ctrl+D fecha a aba da linha, rodapé "Abrir /
  Ações Ctrl+K".
- [ ] Pastas do zoxide (ou, sem zoxide, as pastas das abas) que criam workspace.
- [ ] Prévia colorida da tela (PreviewVt do keepd) no seletor e na busca.
- [ ] Painel de ações (Ctrl+K e botão direito): ir, nova aba, mostrar no
  Explorer, copiar o caminho, fechar; ações de pasta, de linha da busca, de
  comando, de tema e de fonte.
- [ ] Paleta de comandos (Ctrl+Shift+Alt+P e botão redondo): os 17 comandos do
  Mac, em 3 grupos, com as teclas desenhadas e os destrutivos em vermelho.
- [ ] Busca no painel (Ctrl+F, pelo keepd) e em tudo (Ctrl+Shift+F): escopo,
  contador, prévia, ida à linha exata e foco no painel do resultado.

## Fatia `appearance` (temas, fontes, janela)

- [x] Catálogo de temas do Ghostty embutido: 805 de mbadolato/iTerm2-Color-Schemes
  (MIT; licença e créditos em `apps/windows/src/themes/`), gerados por
  `apps/windows/tools/gerar-temas.mjs` e lidos sob demanda. A escolha vale para
  o terminal e para a janela (fundo do tema = ground; barra lateral x0,72 no
  escuro e x0,955 no claro; cartões x0,78 e x0,94; escuro ou claro pela
  luminância), é salva no `state.json` (`theme`; aceita o par `dark:X,light:Y`
  do Ghostty) e vale ao vivo em todos os terminais. "Usar o do Keep" limpa a
  escolha.
- [x] Listas de temas e de fontes (`ui/ThemePicker.tsx`, `ui/FontPicker.tsx`,
  sobre `ui/CatalogPicker.tsx`): busca fuzzy, amostra na linha (o tema com as
  próprias cores; a fonte com `0O l1I {}[] =>`), setas, Ctrl+N/P/J/K,
  PageUp/PageDown, Enter usa, Esc volta ao que estava, a janela veste o que a
  seleção toca. Ligadas nas Configurações; a fatia `palette` abre cada uma com
  `overlay.value = "themes"` ou `"fonts"` e chama `applyTheme`/`clearTheme`/
  `applyFont`/`resetText` de `appearance.ts` nos comandos "Mudar o tema…",
  "Mudar a fonte…", "Usar o tema do Keep" e "Redefinir o tamanho do texto".
- [~] Fontes monoespaçadas: o comando `installed_fonts` (src-tauri `fonts.rs`)
  lê o registro (HKLM e HKCU) mais as pastas de fontes, nomeia cada família
  pela tabela `name` do próprio arquivo (testado no Mac, com fontes reais), e a
  página fica com as que o canvas mede de largura fixa (i, l, ., m, W). O
  código do registro compila para o alvo Windows (`cargo check --target
  x86_64-pc-windows-msvc` de `fonts.rs` isolado) mas só roda no Windows; o
  passo de ponta a ponta de `src/e2eAppearance.ts` confere no CI que o comando
  lista famílias e que a página reconhece uma monoespaçada conhecida.
  Fallback Nerd Font: "Symbols Nerd Font Mono" (MIT) embutida em WOFF2 e
  acrescentada à pilha antes do monospace; os ícones foram vistos na WebView
  do Mac, não na WebView2.
- [x] Cursor: forma (barra, bloco, sublinhado), piscar (padrão sim, como o Mac) e
  oco sem foco; histórico em linhas (padrão 10000, até 100000); tudo ao vivo.
- [x] Título da janela = aba ativa e o workspace (`windowTitle.ts`).
- [~] Modo 2031 (`CSI ? 2031 h` avisa claro/escuro com `CSI ? 997 ; 1|2 n` na
  hora e a cada troca do fundo) e `CSI ? 996 n`; OSC 10/11/12 seguem o tema.
  Visto com um shell de verdade e o keepd no Mac; que o ConPTY deixe passar
  `?2031h` até a página e o relatório de volta até o programa, só o CI do
  Windows mostra.
- [x] Cores da busca da aba (FindBar) derivadas do tema, legíveis no claro e no
  escuro (`Palette.search`, a fatia `palette` pode usar nas outras buscas).
- [ ] Opacidade e desfoque de fundo (Mica/Acrylic): os efeitos exigem janela
  `transparent: true` na criação (todo lançamento, também sem a opção ligada),
  mexem na sombra e nos cantos da janela sem moldura, Mica só existe no
  Windows 11 e Acrylic/Blur travam ao arrastar e redimensionar no Windows 10
  1903+ e no 11; nada disso pode ser provado aqui, e o Mac só lê
  `background-opacity` do Ghostty do usuário (não há opção na tela).

## Fatia `tabs`

Conferida no Mac (roteiro do app, `src/e2eTabs.ts`, com eventos de ponteiro da própria página, e testes
do vitest); o mouse real, os cursores e a dica nativa do workspace não foram exercitados no Windows.

- [x] Reordenar abas por arrasto, na fileira (a aba segue o ponteiro e as outras abrem a lacuna) e na barra
  lateral (cópia na mão, linha apagada no lugar de pouso, lacuna que acompanha), com a ordem no estado
  (`tabOrder`, validada contra o que o keepd lista e esquecida se o keepd for outro).
- [x] Mover aba (com os painéis) para outro workspace arrastando na barra lateral (`api.moveTab`): o terminal é
  religado, não refeito, e o nome, o lugar, o painel com o teclado e o estado do Claude vão junto. Soltar
  no cabeçalho põe no fim; soltar sobre uma aba põe antes dela.
- [x] Arrastar o cabeçalho do workspace reordena a lista, com a mesma lacuna.
- [x] Alça do painel (44x14, três pontos, só com mais de um painel): cartão com o texto do painel, pouso nos
  quatro lados e na troca (`api.rearrange`), soltar numa aba da fileira junta o painel a ela, soltar fora o
  destaca em aba nova, 0,45 s sobre uma aba a abre (a mola). Trocar dois painéis mantém o layout e troca
  os lugares (também entre abas); o herdeiro assume o nome e o lugar da aba.
- [x] Divisor arrastável (razão em memória, 50/50 no início, mínimo de 12 colunas e 4 linhas, duplo clique volta
  ao meio) e selo "cols × rows" enquanto o painel muda de tamanho.
- [x] Foco por geometria (Alt+setas) e redimensionamento (Alt+Shift+setas, 20 px) só em abas com painéis; nas
  outras as setas são do programa. Documentados no menu do painel.
- [x] Abas verticais: `toggleVerticalTabs` (store.ts), no estado, e no menu de contexto da fileira. O registro
  do comando na paleta fica com a fatia `palette` (não há `commands.ts` neste ramo).
- [x] Marca de painéis na aba (na fileira e na barra lateral) e dica do workspace (nome real, o que roda ou
  "N abas · estado").
- [x] Renomear sem fixar o título atual (só grava o que mudou), um só campo de nome por vez, clique lento no
  nome da aba ou do workspace em frente (meio segundo, uma tecla ou outro clique cancelam), F2 pergunta o
  nome quando nada mostra as abas; o herdeiro herda o nome ao fechar a raiz.
- [x] Workspace padrão com o nome do usuário quando o keepd está vazio (campo `user` em `startup`).
- [x] Hibernação: no máximo 6 abas com terminal (a em frente nunca; seleção, teclado ou composição seguram a
  aba); a rolagem é guardada e o keepd repinta a tela. O que rola para fora da tela enquanto a aba dorme
  não entra na rolagem local (o keepd tem; a busca em tudo acha). Como no Mac, uma aba ocupada também
  dorme. As adormecidas seguem lidas pela tela do keepd (`preview`); quem agir sobre o terminal de uma aba
  que acabou de abrir deve esperar por `viewWhenAwake` (store.ts), por exemplo ir à linha de um resultado.
- [x] Workspaces guardados: "Remover da barra lateral" no menu do workspace (`putAway`, lista `hidden` no
  estado), o Ir para (`select`) devolve; só vale enquanto o workspace roda. O comando "Remover este
  workspace da janela" fica para o registro de comandos da paleta.

## Fatia `terminal`

- [ ] Caminhos de arquivo clicáveis (relativos ao cwd, ~, :linha:col, quebrados
  em duas linhas), clique abre; menu Abrir / Mostrar no Explorer / Copiar.
- [ ] URLs mailto: e file://; Ctrl+clique.
- [ ] Endereços web: clique simples abre no navegador, inclusive o link OSC 8
  atrás do texto (markdown do Claude Code) e o endereço quebrado em linhas;
  menu Abrir link / Copiar link (Mac: 0e8c0bc). Com o mouse tomado pelo
  programa (Claude Code em tela cheia), o clique simples deixa o endereço
  para ele, que já o abre (Mac: 62be02e).
- [ ] OSC 52 com política; confirmar colagem multi-linha; colar arquivo copiado.
- [ ] Soltar arquivos: realce do painel sob o ponteiro.
- [ ] Teclado kitty (CSI u): Ctrl+Enter, desambiguação.
- [ ] Cut multi-linha no prompt do Claude.
- [ ] Título OSC imediato (onTitleChange); grafemas Unicode (modo 2027).

## Fatia `ai`

- [ ] Integração de shell (função `claude` no PowerShell): troca a quente,
  armazém por aba.
- [ ] Restauro depois de reiniciar, com as conversas (keep restauro), sem o
  retrato apagado cedo.
- [ ] `keep continuar` a cada 15 s.
- [ ] Responder "confiar nesta pasta?" sozinho.
- [ ] Ganchos (hooks) do Claude e do Codex.
- [ ] Rótulo "(IA·1→2)" e o nome da aba preservado na troca.
- [ ] `keep nova` e links claude-cli://.
- [ ] Robustez do state.json (cópias, JSON inválido avisado).

## Fatia `confine`

Restringir a IA de um workspace a uma pasta. Opt-in: sem pasta escolhida, nada
muda. A escolha fica em `%USERPROFILE%\.config\keep\confinement.json` (o
formato do Mac) e quem prende é o `keep ia confinar -- <programa>`.

- [x] Botão de pasta no cabeçalho do workspace (aberta: livre; cheia: presa),
  menu do botão ("A IA só acessa …", "Trocar a pasta…", "Liberar a IA") e itens
  do menu do cabeçalho ("Restringir a IA a uma pasta…", "Liberar a IA (hoje:
  …)"), com o diálogo de pasta do sistema. Vale para as sessões iniciadas
  depois, e o tooltip diz, bem como diz que o Git Bash não roda preso. Recusa a
  raiz de uma unidade e a pasta do usuário. Testado (vitest, `confinement.rs`)
  e visto na tela do app.
- [x] A contenção: token restrito (`CreateRestrictedToken`) com permissões só na
  pasta, no git dela e no mínimo de que a IA precisa, mais os atributos (sem
  listar) das pastas no caminho até elas, privilégios apagados, Job que mata
  junto, prova do token antes de subir (o de fora é negado, a pasta funciona) e
  falha fechada (`crates/keep-ia/src/confinar_win*`). Conferida no CI do
  Windows (workflow Núcleo), com o token elevado do CI e com o de quem não
  está elevado (`keep ia confinar --como-comum`, o do UAC ligado): o autoteste
  (`keep ia confinar --autoteste`), o teste `o_programa_preso_so_toca_na_pasta_dele`,
  o programa preso dentro de um ConPTY de verdade, com a saída, a entrada
  digitada e o código de saída (`crates/keep/tests/confinar_pty.rs`), e o
  roteiro `tools/confine-test.ps1`: `cmd`, `pwsh`,
  `powershell`, `git`, `node` (com o `localhost`) e o Claude Code, que só
  leem, listam e gravam o que a pasta permite. `keep ia confinar --diagnostico`
  mostra o que o Windows aceita do token. `CONFINEMENT_ENFORCED`
  (confinement.ts) tira o botão e os itens do menu.
- [x] O Claude preso sem o Git Bash, que não roda preso: o runtime do MSYS2 cria
  um canal de sinais com nome e uma DACL só do usuário e do sistema, e o token
  restrito não leva nenhum dos dois (`couldn't create signal pipe`, erro 5),
  com ou sem outro bash aberto. O Claude Code usa o Git Bash para a ferramenta
  Bash; preso, o Keep a nega por um arquivo de configuração (`--settings`, em
  `%LOCALAPPDATA%\Keep\confine`) e liga a ferramenta PowerShell
  (`CLAUDE_CODE_USE_POWERSHELL_TOOL=1`). Conferido no CI: o Claude preso lista
  `PowerShell` e não lista `Bash`.
- [~] O Claude Code logado, de verdade, preso: o CI não tem login. Conferido até
  a lista de ferramentas e o pedido de login; não conferido o PowerShell dele
  rodando comandos, os ganchos e a linha de status (que ele roda pelo Git Bash
  quando existe) e a gravação de `.claude.json` (o arquivo temporário e a
  trava ficam na casa, que a IA não pode gravar). O Gerenciador de Credenciais
  não funciona preso (conferido no CI: não vê as credenciais do usuário e não
  grava, "Credentials cannot be saved from this logon session"): o Claude Code
  só guarda o login nele quando uma opção do servidor está ligada, e cai para
  o arquivo `.credentials.json` da `.claude`, que a IA alcança; com a opção
  ligada, preso ele não vê o login do usuário e pede outro. O Codex só tem o
  encaixe (`--no-daemon`): sem Codex no CI.
- [~] Encaixe na função `claude` do PowerShell (fatia `ai`): trocar o
  `& $claude @args` final por `& $keep ia confinar -- $claude @args`; sem pasta
  para o workspace o `confinar` roda o programa como sempre. Enquanto a função
  não passa por ele, só a troca de conta/IA e a restauração (que já chamam
  `envolve`) prendem; digitar `claude` no shell, não.
- [ ] O que o Windows não deixa fechar sem administrador: pastas de rede (o
  servidor confere o usuário, não o token), o que o Windows deixa qualquer um
  ler (`C:\Windows`, `Arquivos de Programas`, o que `Usuários` lê em outras
  unidades, `C:\Users\Public`) ou gravar (`C:\ProgramData`, `C:\Windows\Temp`),
  o registro do sistema, a área de trabalho (o programa preso pode mandar
  teclas a outras janelas), o WSL. Programas instalados só para o usuário
  (nvm, cargo, pyenv, Python do usuário) ficam de fora, como o `~/.nvm` no Mac;
  o Gerenciador de Credenciais do Git também. Um arquivo movido (não copiado)
  de fora para dentro da pasta presa leva a permissão que tinha, e a IA não o
  abre. As permissões dadas (as pastas, e os atributos das pastas do caminho)
  ficam quando a pasta é trocada ou liberada. Uma alternativa mais fechada,
  que não tem esses limites nem o do Git Bash, é um usuário dedicado do
  Windows para a IA (como o sandbox do Claude Code), que pede administrador
  uma vez.

## Fatia `windows` (várias janelas)

- [ ] Nova janela; lista de workspaces, aba e painel por janela; mover
  workspace entre janelas; arrancar a aba da faixa.
- [ ] Janela rápida global (Ctrl+`), com o workspace `quick` oculto.
- [ ] Estado das janelas (quadro, tela cheia).
