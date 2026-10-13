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

Um cartão só (ui/Picker.tsx), como o PickerView do Mac, com quatro perguntas:
Ir para (Ctrl+Shift+P), Comandos (Ctrl+Shift+Alt+P), Buscar neste painel
(Ctrl+F) e Buscar em tudo (Ctrl+Shift+F). A barra de busca do xterm ficou em
Ctrl+Alt+F e no menu do painel ("Procurar na tela").

Conferida no Mac pelo roteiro do app (`src/e2ePalette.ts`, cinco passos contra
um keepd de verdade) e à mão, nos temas escuro e claro, com a prévia, as ações,
a busca e as listas de temas e de fontes; as teclas no WebView2 e o Explorer
não foram vistos no Windows de verdade.

- [x] Ir para: recentes primeiro (a 1ª linha é a última aba de OUTRO
  workspace), cabeçalhos "terminais" e "abrir em", linha rica, busca em colunas
  com as letras acesas, ↑↓ / Ctrl+J / Ctrl+N / Ctrl+P, duplo clique escolhe,
  Ctrl+D fecha a aba da linha (com a pergunta e a nota das worktrees), rodapé
  "Abrir / Ações Ctrl+K". Ctrl+K abre as ações (no Mac é ⌘K; o ⌃K de subir da
  lista do Mac colide com ele aqui, e subir fica em ↑ e Ctrl+P).
- [~] Pastas do zoxide (ou, sem zoxide, as pastas das abas e a pasta pessoal)
  que criam workspace. Testado no Mac; no Windows só compilado (a janela do
  `zoxide` escondida é `#[cfg(windows)]`).
- [x] Prévia colorida da tela (PreviewVt do keepd) no seletor e na busca: um
  xterm só para olhar, na grade que o keepd guarda; na busca, o painel do
  resultado com a linha achada e o trecho em destaque.
- [~] Painel de ações (Ctrl+K e botão direito): ir, nova aba, mostrar no
  Explorer, copiar o caminho, fechar; ações de pasta, de linha da busca e de
  comando. Testado no Mac; "Mostrar no Explorer" no Windows só compilado.
- [ ] Ações de tema e de fonte (Ctrl+K numa linha das listas: "Vestir este
  tema" / "Usar esta fonte" e "Copiar o nome"): as listas são as da fatia
  `appearance` (`ui/CatalogPicker.tsx`), cartões autônomos em que Ctrl+K sobe a
  seleção e que não têm painel de ações; dá-lo a elas é mexer no teclado dessa
  fatia, e Enter e Esc já fazem o essencial.
- [x] Paleta de comandos: os 17 do Mac em 3 grupos, com as teclas desenhadas e
  os destrutivos em vermelho, mais Configurações, Tela cheia e Colar imagem. O
  botão redondo da fileira de abas virou "Comandos" (a engrenagem é um comando
  e segue em Ctrl+,). Ligados a funções das outras fatias em
  `palette/bindings.ts`: "Alternar abas verticais" (`toggleVerticalTabs`),
  "Remover este workspace da janela" (`putAway`), "Usar o tema do Keep"
  (`clearTheme`) e "Redefinir o tamanho do texto" (`resetText`, que também
  desfaz o zoom e a fonte).
- [x] "Mudar o tema…" e "Mudar a fonte…" põem a lista da fatia `appearance`
  (`ThemePicker`, `FontPicker`) no lugar do cartão da paleta: a seta veste ao
  vivo, Enter usa e fecha tudo, Esc volta à paleta (um passo para trás, como
  no Mac) e desfaz o que se viu.
- [ ] "Nova janela" fica cinza na paleta até a fatia `windows` ligá-la com
  `registerCommand` (commands.ts), que vale mais que qualquer padrão da janela.
- [x] Busca no painel (Ctrl+F, pelo keepd) e em tudo (Ctrl+Shift+F): escopo,
  contador "3 de 47", prévia, ida à aba, ao painel e à linha exata (contada do
  fim, como o Mac), com a linha marcada por instantes nas cores de busca do
  tema em uso (`Palette.search`, legíveis em qualquer fundo). Numa aba
  adormecida (hibernação) espera o terminal por `viewWhenAwake` antes de ir à
  linha.

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

- [~] Integração de shell (função `claude` e `codex` no PowerShell das abas): troca a quente, armazém por
  aba, e a subida pelo `keep ia confinar` (a pasta presa da fatia `confine`: sem pasta para o workspace o
  `confinar` roda o programa como sempre, e se o keep recusa prender nada sobe solto). Feita pelo keepd
  (`crates/keepd/src/ai.ps1`, `shellhook.rs`); testada com um `pwsh` 7 de verdade e programas de mentira
  (argumentos, pipe, código de saída, falha segura, subcomandos, função da pessoa, o `confinar` que recusa,
  o `claude.ps1` na frente no PATH), e conferida para o Windows com `cargo check --target
  x86_64-pc-windows-msvc`; não rodou num Windows nem no PowerShell 5.1. Cmd e Git Bash ficam de fora
  (rodam no login global e soltos) e o WSL é invisível ao crate (docs/ia.md). O `--` digitado sem aspas é
  consumido pelo PowerShell antes de qualquer função (`claude -- texto` chega como `claude texto`).
- [~] Restauro depois de reiniciar, com as conversas (`keep restauro`), sem o retrato apagado cedo: o app
  deixou de recriar abas em JS; `restaurar --fase=abas` antes de mostrar o vazio (espera até 1 min por um
  keepd lento), `manter` a cada 30 s, nomes das abas pelo `names.json`, trava do crate que agora trava no
  Windows. Visto no app com um keepd de teste no Mac (keepd morto e app aberto sobre keepd morto); o que é
  só do Windows (`LockFileEx`, subida das conversas no PowerShell) compila e não rodou (docs/restauro.md).
- [~] `keep continuar` a cada 15 s, com o ocioso do sistema pelo `GetLastInputInfo`.
- [ ] Responder "confiar nesta pasta?" sozinho (fatia `terminal`).
- [~] Gancho da conta da aba no Claude (`UserPromptSubmit` -> `keep ia conta-da-aba --gancho` no
  `settings.json`, acrescentado depois dos ganchos da pessoa, sem mexer em mais nada, sem repetir): o plano
  é puro e testado (formatos, idempotência, ganchos alheios intactos); o efeito dentro do Claude Code no
  Windows (Git Bash ou cmd rodando o comando) não foi visto.
- [-] Gancho do Keep no Codex: o Mac também não instala nenhum (o `hooks.json` do Codex é do Jev) e a
  conta de uma aba do Codex não muda enquanto ele roda.
- [x] Rótulo "(IA·1→2)" até o Claude responder na conta nova (a `provada` do `keep ia abas`) e o nome da
  aba preservado na troca de conta.
- [~] `keep nova` no Windows: `--nome` vale (pelo `names.json`), o `canonicalize` sem o prefixo `\\?\`, e o
  `keep` no PATH de toda aba (e em `KEEP_IA_BIN`).
- [ ] Links `claude-cli://` numa aba do Keep: o Claude Code no Windows abre o terminal pelo `wt.exe`,
  `pwsh.exe`, `powershell.exe` ou cmd, sem olhar `TERM_PROGRAM` nem `deepLinkTerminal`, e refaz o registro
  do protocolo toda sessão; o desvio (um `wt.exe` do Keep + registro + `disableDeepLinkRegistration`) mexe
  no registro e no `settings.json` e não se prova fora do Windows (docs/restauro.md). O `<instalação>\bin`
  no PATH do usuário pelo instalador também ficou de fora, pelo mesmo motivo (a aba já o tem).
- [x] Robustez do `state.json`: cópias rotativas (`.bak1` a `.bak5`, uma a cada 10 min), recuperação da
  última boa, o arquivo ilegível guardado em `.ilegivel` e o aviso na janela; fsync antes do rename; nada é
  gravado por cima de um arquivo que não deu para ler ao abrir.
- [ ] `arquivos_abertos` no Windows (id da thread do Codex pelos arquivos abertos): o Restart Manager ou o
  `NtQuerySystemInformation` dariam o dado, mas desde o Codex 0.159 os locks são do daemon dele, não da tela
  (o ganho é pouco) e nada disso se prova daqui; fica como está, dito em `processos.rs`.

## Fatia `confine`

Restringir a IA de um workspace a uma pasta. Opt-in: sem pasta escolhida, nada
muda. A escolha fica em `%USERPROFILE%\.config\keep\confinement.json` (o
formato do Mac) e quem prende é o `keep ia confinar -- <programa>`.

- [x] Botão de pasta no cabeçalho do workspace (aberta: livre; cheia: presa),
  menu do botão ("A IA só acessa …", "Trocar a pasta…", "Liberar a IA") e itens
  do menu do cabeçalho ("Restringir a IA a uma pasta…", "Liberar a IA (hoje:
  …)"), com o diálogo de pasta do sistema. Vale para as sessões iniciadas
  depois, e o tooltip diz. Recusa a raiz de uma unidade e a pasta do usuário.
  Testado (vitest, `confinement.rs`) e visto na tela do app.
- [~] A contenção: token restrito (`CreateRestrictedToken`) com permissões só na
  pasta, no git dela e no mínimo de que a IA precisa, Job que mata junto, prova
  do token antes de subir (o de fora é negado, a pasta funciona) e falha
  fechada (`crates/keep-ia/src/confinar_win*`). Só compilada para o Windows
  daqui; o CI roda o autoteste (`keep ia confinar --autoteste`, também
  dentro de uma aba) e `tools/confine-test.ps1` repete o roteiro à mão.
  `CONFINEMENT_ENFORCED` (confinement.ts) tira o botão e os itens do menu.
- [~] Encaixe nas funções `claude` e `codex` do PowerShell (fatia `ai`, feito):
  elas sobem o programa por `keep ia confinar -- <programa> …` em toda aba do
  Keep, como o `_keep_ia` do Mac, e não caem para a subida solta quando o keep
  recusa; sem pasta para o workspace o `confinar` roda o programa como sempre.
  Testado com um `pwsh` 7 e um `confinar` de mentira (docs/ia.md); digitar
  `claude` numa aba do Windows com a pasta presa não foi visto. Um `claude.ps1`
  sozinho (sem `.exe` nem `.cmd` ao lado), o Cmd e o Git Bash rodam sem prender.
- [ ] O que o Windows não deixa fechar sem administrador: pastas de rede (o
  servidor confere o usuário, não o token), o que o Windows deixa qualquer um
  ler (`C:\Windows`, `Arquivos de Programas`, e, nos níveis compatível e amplo,
  que valem se o estrito não deixa o programa partir, o que `Usuários` lê em
  outras unidades), o registro do sistema, a área de trabalho (o programa preso
  pode mandar teclas a outras janelas), o WSL. Programas
  instalados só para o usuário (nvm, cargo, pyenv) ficam de fora, como o
  `~/.nvm` no Mac; o Gerenciador de Credenciais do Git também. Um arquivo
  movido (não copiado) de fora para dentro da pasta presa leva a permissão que
  tinha, e a IA não o abre.

## Fatia `windows` (várias janelas)

- [ ] Nova janela; lista de workspaces, aba e painel por janela; mover
  workspace entre janelas; arrancar a aba da faixa.
- [ ] Janela rápida global (Ctrl+`), com o workspace `quick` oculto.
- [ ] Estado das janelas (quadro, tela cheia).
