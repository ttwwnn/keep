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
- [x] "Nova janela" na paleta: ligada pela fatia `windows` com `registerCommand` (windowSync.ts), com a tecla
  Ctrl+Shift+Alt+N desenhada.
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
- [~] Opacidade e desfoque de fundo (fatia `glass`; opt-in, desligado por padrão). Configurações > Fundo
  translúcido (só no Windows; o Mac lê o `background-opacity` do Ghostty do usuário): opacidade de 50 a 100% (100% é
  o app de sempre) e desfoque Nenhum/Mica/Acrílico, só o que o build oferece (Mica a partir do 22621, Acrílico a
  partir do 17763; o texto do pedido dizia 1803, mas o window-vibrancy só aplica a partir do 1809). Guardado no
  estado (`glass`, compartilhado por todas as janelas pelo mesmo canal dos outros campos; estado antigo sem o campo
  lê como opaco). Decisões:
  - `transparent` só vale na criação da janela. Fixá-lo sempre em `true` mudaria a janela de quem nunca ligou a
    opção (o tao passa a usar o blur-behind do DWM e uma superfície própria) sem que eu possa ver o resultado; então
    a janela principal é criada em `glass::open_main` (a configuração tem `create: false`) e só sai transparente se o
    estado pede opacidade menor que 100% (`wants_translucency`, testado). Ligar a opção num app em andamento mostra
    "Reiniciar o Keep" (`restart_app`: o app novo espera o PID do antigo sair antes de montar o
    single-instance; o keepd e as abas não são do app e continuam). As janelas seguintes (`windows::build`) saem
    como a primeira.
  - O material (Mica, Acrílico) é aplicado em tempo de execução com `set_effects` nas janelas transparentes
    (`glass_apply`; limpa o anterior antes; MicaDark/MicaLight pela cor do fundo, sem `set_theme`, que mexeria no
    `prefers-color-scheme`). Quem nunca pediu material não chama nada do DWM.
  - Na página (`html.glass`, `styles-glass.css`, só com alfa < 1): o corpo e o `#app` ficam transparentes, a barra
    lateral, a barra de título, a barra de IA e o estado vazio pintam `--recessed-glass`/`--ground-glass` (o
    `--ground` de sempre com o alfa), o xterm ganha `allowTransparency` e o tema com `background` em rgba (só o
    fundo padrão; as células que um programa pinta ficam sólidas, como no Ghostty) e o que o terminal não cobre
    (2 px de cada lado, sobra de célula) é pintado por uma `box-shadow` do `.xterm-screen`, para o fundo nunca ser
    pintado duas vezes. Texto, cartões (`--overlay`) e campos não ficam translúcidos. Com um seletor, a paleta, o
    seletor de tema ou de fonte abertos a janela fica opaca (o `holdOpaque` do Mac); as Configurações não seguram,
    para ver a barra mexer o fundo.
  - Acrílico no Windows 10 pode travar ao arrastar ou redimensionar: a opção avisa.
  Provado aqui: vitest (`glass.test.ts`: normalização e migração, material por build, alfa, rgba do tema e das
  superfícies), `cargo test` (`glass::tests`), `cargo check` e `cargo check --target x86_64-pc-windows-msvc`.
  Só o CI do Windows prova: o roteiro (`e2eGlass.ts`) confere o padrão opaco, o pedido numa janela opaca, a classe e as
  cores no webview, a janela rápida transparente e os materiais pedidos e retirados sem erro. Não visto por nenhum
  olho: o desfoque real, a sombra e os cantos de uma janela transparente sem moldura, o redimensionar e o arrastar
  com Acrílico, o botão "Reiniciar o Keep" de ponta a ponta no Windows.

## Fatia `tabs`

Conferida no Mac e no Windows do CI (roteiro do app, `src/e2eTabs.ts`, com eventos de ponteiro da própria
página, e testes do vitest); o mouse real, os cursores e a dica nativa do workspace não foram exercitados no
Windows. A janela do CI deixa 286 px para a lista da barra lateral: o roteiro mostra a linha (`reveal`) antes de
puxá-la, como uma pessoa faz, e o log do app traz o que a página via em cada arrasto (elemento sob o ponteiro,
caixa da linha, rolagem da lista, estado do movimento, fim do arrasto por `pointerup`, `blur` ou Esc).

- [x] Reordenar abas por arrasto, na fileira (a aba segue o ponteiro e as outras abrem a lacuna) e na barra
  lateral (cópia na mão, linha apagada no lugar de pouso, lacuna que acompanha), com a ordem no estado
  (`tabOrder`, validada contra o que o keepd lista e esquecida se o keepd for outro).
- [x] Mover aba (com os painéis) para outro workspace arrastando na barra lateral (`api.moveTab`): o terminal é
  religado, não refeito, e o nome, o lugar, o painel com o teclado e o estado do Claude vão junto. Soltar
  no cabeçalho põe no fim; soltar sobre uma aba põe antes dela.
- [x] Arrastar o cabeçalho do workspace reordena a lista, com a mesma lacuna.
- [x] Lista mais longa que a janela: com o ponteiro a menos de 36 px da borda de cima ou de baixo a lista rola
  sozinha, mais depressa quanto mais perto (no máximo 14 px por quadro, também além da borda), o pouso é
  refeito a cada rolagem mesmo com o ponteiro parado, e soltar faz o que estava mostrado (`drag.ts`
  `scrollStep`/`autoScroll`, `ui/tabDrag.ts`). Antes, a velocidade crescia sem limite além da borda e a soltura
  refazia a conta com a lista já rolada: a aba voltava sem mudar de lugar.
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

- [x] Caminhos de arquivo clicáveis (`paths.ts`, `links.ts`, `links.rs`): `C:\x\y.txt`, UNC, `\\wsl$`, `.\rel`,
  `..\rel`, `/mnt/c/...`, `~/x`, relativos ao cwd do painel, `:linha` e `:linha:coluna` (e `(linha,coluna)`),
  quebrados em linhas pelo ajuste, com espaço escapado ou entre aspas. Só vira link o que o sistema confirma
  (`find_paths`: 2 s no máximo, sem nomes de dispositivo). 26 testes; visto no app com eventos de mouse reais.
- [x] Endereços web (`urls.ts`): https, http, ftp, mailto, `www.` e `file:` (que vira caminho), pontuação do fim
  da frase fora, link quebrado em até três linhas pela linha que vai até a borda, e o OSC 8 atrás do texto.
  17 testes; visto no app.
- [x] Clique simples (500 ms depois do duplo clique, sem arrasto, painel já com o foco) ou Ctrl+clique abrem;
  sublinhado sempre sob o ponteiro; botão direito: sobre o caminho Abrir / Mostrar no Explorer / Copiar o
  caminho, sobre o endereço Abrir link / Copiar link, e nada disso onde não há nada sob o ponteiro.
- [~] O que o Windows faz com o clique: `open_path`, `reveal_path` e `open_url` (o plugin opener: ShellExecute e
  `SHOpenFolderAndSelectItems`) compilam para o Windows (`cargo check --target x86_64-pc-windows-msvc`) e não
  rodaram nele; executáveis e scripts são mostrados no Explorer em vez de executados (o Mac faz o mesmo), e
  `open_url` recusa o que não é http, https, mailto ou ftp (o Mac entrega `vscode://` e `tel:` ao sistema no
  ⌘-clique; aqui não: texto de programa não inicia programa). Diferença do Mac: com o mouse tomado pelo programa (Claude Code em
  tela cheia) o Mac deixa o endereço para ele (62be02e), que o abre porque o terminal se diz ghostty; aqui o
  terminal se diz `TERM_PROGRAM=keep`, então o clique simples continua abrindo. Não visto com o Claude Code no
  Windows.
- [x] OSC 52 (`osc52.ts`): escrita permitida só no `c` (`s` e `p` descartados); leitura pergunta primeiro
  ("Permitir a leitura da área de transferência?", com prévia) e responde `ESC ] 52 ; c ; base64 ESC \`. 12
  testes; visto no app.
- [x] Colar (`paste.ts`): sem o bracketed paste, qualquer quebra de linha pergunta ("Colar isto?", prévia de 400
  caracteres; o Mac só conta `\n`, aqui `\r` também); com ele, só um `ESC [ 201 ~` dentro do texto pergunta;
  os controles viram espaço. Testes e visto no app.
- [ ] Colar arquivo copiado no Explorer (digitar o caminho): exigiria ler o `CF_HDROP` da área de transferência
  do Windows, código `unsafe` do Win32 que só compila aqui e não se prova; o Mac também só lê texto.
- [~] Soltar arquivos (`drop.ts`): o painel sob o ponteiro se realça e os caminhos são digitados como o shell
  do painel os lê (PowerShell: entre aspas, ou aspas simples se há `$`, crase ou `'`; WSL: o caminho Linux,
  `/mnt/c/...`, escapado como o Mac; outros: aspas duplas se há espaço). Lógica testada; o arrasto do sistema
  não se exercita daqui (o realce e o drop reais ficam para o Windows).
- [~] Teclado Kitty (`kitty.ts`, `kittyKeys.ts`): o programa liga o protocolo (`CSI > flags u`, pilha de oito por
  tela, `CSI < n u`, `CSI = flags ; modo u`, `CSI ? u` respondida) e as teclas saem como ele pediu: Shift+Enter,
  Ctrl+Enter, Esc, Ctrl+Shift+letra, setas e F-teclas com modificadores, eventos, alternativas, todas as
  teclas e texto associado; é o codificador do Ghostty (os seus testes, menos os de CapsLock e NumLock, que o
  Mac nunca informa) e as duas escolhas do Mac, Shift+Tab sempre `ESC [ Z` e Alt+Backspace sempre `ESC DEL`.
  O texto digitado, o AltGr, a composição e as teclas mortas ficam com o xterm.js; sem o protocolo nada muda e
  Shift+Enter segue mandando `ESC CR`. Visto no app (Mac) com uma sonda que liga as flags 5 (as do Claude
  Code) e 31: todas as teclas acima, o `CSI = n ; 1 u` do repaint devolvendo as flags a um app reaberto. No
  Windows resta o que só um ConPTY mostra: lendo o OpenConsole (`main`), a saída do programa vai ao terminal
  como foi escrita e uma sequência de teclas que ele não conhece chega ao programa como texto; não rodou. O
  passo "teclado Kitty" do roteiro do CI (`e2e/sonda-teclado.mjs`) confere nos dois sentidos, com um Node de
  verdade no ConPTY; se ele falhar, o ConPTY não deixa passar `CSI > u` ou `CSI ? u`.
- [~] Cortar (Ctrl+X) na caixa de prompt do Claude com várias linhas e na seleção que o próprio programa copiou
  (`cut.ts`): o CutSelection do Mac, testado com as oito gravações reais da caixa (`tools/cut-test`); a tecla
  no app não foi exercitada.
- [x] Título OSC imediato: `onTitleChange` pede a lista ao keepd em 120 ms (`refreshSoon`), em vez de esperar o
  giro do segundo; visto no app em 0,3 s, e o roteiro do CI mede.
- [ ] Grafemas Unicode (modo 2027): o `@xterm/addon-unicode-graphemes` 0.4.0 existe para o xterm 6.0.0, mas
  como "experimental", o xterm 6.0.0 não trata o modo 2027 e o grid do keepd só agrupa grafemas quando o
  programa o liga; ligado fixo, desalinharia as linhas. Pede uma dependência nova (o `node_modules` é de
  todas as árvores) e `CSI ? 2027 h/l` e a consulta DECRQM tratados na página, e não se prova daqui.
- [~] Responder sozinho "Confiar nesta pasta?" do Claude Code (`trust.ts`): a seta até o "Yes, I trust this
  folder" e, quando o próximo olhar a encontra lá, Enter; nunca Enter no "No, exit". O layout antigo e o
  numerado; o marcador `~/.local/state/keep/trust/<pid do shell>` do vigia do shell evita a seta dupla. Visto
  no app (Mac) com um `claude` de mentira que desenha a pergunta; o Windows (o marcador com `\`) e o Claude
  Code de verdade não.
- [x] Verificação no roteiro do CI (`e2eTerminal.ts`): o protocolo pelo PTY (acima), o título que o programa
  põe chegar à aba e os caminhos que o sistema confirma (pasta, arquivo, ausente, nomes de dispositivo). Os
  três passaram no Mac com o roteiro inteiro; no Windows quem diz é o CI.

## Fatia `ai`

- [~] Integração de shell (função `claude` e `codex` no PowerShell das abas): troca a quente, armazém por
  aba. Feita pelo keepd (`crates/keepd/src/ai.ps1`, `shellhook.rs`); testada com um `pwsh` 7 de verdade e
  programas de mentira (argumentos, pipe, código de saída, falha segura, subcomandos, função da pessoa),
  e conferida para o Windows com `cargo check --target x86_64-pc-windows-msvc`; não rodou num Windows nem
  no PowerShell 5.1. Cmd e Git Bash ficam de fora (rodam no login global) e o WSL é invisível ao crate
  (docs/ia.md). O `--` digitado sem aspas é consumido pelo PowerShell antes de qualquer função (`claude --
  texto` chega como `claude texto`).
- [~] A subida presa a uma pasta (`keep ia confinar --`, o `_keep_ia` da função do Mac): LIGADA, porque a
  contenção passou no CI do Windows (`CONFINEMENT_ENFORCED = true`; fatia `confine`). Um `confinar` de mentira e
  o `pwsh` testam a rota (nunca solta quando o keep recusa, o `claude.exe` ou o `claude.cmd` e não o
  `claude.ps1`, `agents`/`attach` presos, o resto direto); desligar é trocar `{ $true }` por `{ $false }` na
  linha `__KeepConfining` de `ai.ps1`, com o `CONFINEMENT_ENFORCED`.
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
- [~] Encaixe nas funções `claude` e `codex` do PowerShell (fatia `ai`): LIGADO
  (`__KeepConfining` em `crates/keepd/src/ai.ps1`, com o `CONFINEMENT_ENFORCED`). As duas sobem por
  `keep ia confinar -- <programa> …` em toda aba do Keep, como o `_keep_ia` do Mac, sem cair para a subida
  solta quando o keep recusa; sem pasta para o workspace o `confinar` roda o programa como sempre. Testada
  com um `pwsh` 7 e um `confinar` de mentira (docs/ia.md) e, no CI, com o `keep ia confinar` de verdade;
  digitar `claude` numa aba do Windows com a pasta presa não foi visto. Um `claude.ps1` sozinho (sem `.exe`
  nem `.cmd` ao lado), o Cmd e o Git Bash rodam sem prender.
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

## Fatia `quick` (janela rápida)

O terminal que desce do topo da tela com um atalho global (o ⌃` do Mac). A janela é a metade passageira: o
shell é uma aba do workspace `quick` do keepd, que sobrevive à janela, ao app e ao login, e que nenhuma outra
parte da janela lista. Código: `src-tauri/src/quick.rs` (atalho, janela, deslize), com `quick/geometry.rs` (as
medidas e as curvas) e `quick/shortcut.rs` (o atalho e a tecla do layout) à parte para serem testados sem janela;
`src/quick/` (a página do painel, o teclado dele, o que a janela principal mostra), `src/quickWorkspace.ts` (o
filtro) e `capabilities/quick.json`.

Conferida no Mac pelo roteiro do app (`src/e2eQuick.ts`, sete passos contra um keepd de verdade: atalho,
geometria, workspace oculto, shell vivo, volta ao mesmo shell, troca do atalho) e à mão, com um atalho de teste
apertado por tecla de verdade pelo HID: a entrada, a saída, o clique fora, o shell que acaba, o tema e o zoom
lidos do estado, a paleta, as Configurações, o atalho recusado e a sugestão. O que é só do Windows foi
compilado (`cargo check --target x86_64-pc-windows-msvc`) e não rodou num Windows; o CI confere o que a janela
real pode dizer de si (o atalho entregue e a geometria; o foco não: uma máquina sem ninguém na frente pode não
dá-lo).

- [x] O workspace `quick` é invisível para o resto: sai de `api.list` e `api.search` (`quickWorkspace.ts`), e
  portanto da barra lateral, do Ir para, da paleta, da busca em tudo, da ordem e do restauro (um keepd só com ele
  conta como vazio para o `restoreIfEmpty`, pelo código: não exercitado); o retrato do core já o ignorava (`RAPIDO`). Visto no Mac: a barra lateral
  sem ele, `keep ls` com ele, `state.json` sem uma palavra sobre ele.
- [~] Atalho global Ctrl+` (`tauri-plugin-global-shortcut`, `RegisterHotKey`). No Windows o atalho é registrado
  pela tecla que o layout em uso dá àquele lugar do teclado (`MapVirtualKeyW` por scancode, depois a tabela do
  `global-hotkey`, que é a do layout US), para que a tecla à esquerda do 1 seja a mesma em US, ABNT2, alemão e
  AZERTY, e uma letra seja a do lugar, não a que digita (como o `kVK_ANSI_Grave` do Mac). A conta é pura e
  testada com layouts de mentira; a chamada ao sistema e o que ela devolve num ABNT2 de verdade não foram vistos
  (a suposição é que o thread tem o layout em uso).
- [x] Configurável em Configurações, Janela rápida: Alterar (aperta o novo), Usar o padrão, Abrir agora. Fica em
  `quick.json` ao lado do `state.json` (só o que a pessoa escolheu). Recusa Esc e um atalho sem Ctrl, Alt ou Win
  (a não ser tecla de função); o novo é tomado antes de soltar o antigo, então um recusado deixa tudo como estava.
- [x] Atalho recusado pelo sistema (outro programa o tem): aviso ao abrir o app, o motivo nas Configurações e um
  atalho livre para pôr no lugar (Ctrl+Shift+`, Ctrl+Alt+`, Ctrl+Shift+K, Ctrl+Alt+Espaço: o que se pode tomar
  agora, provado tomando e devolvendo), com um botão que o usa. Visto no Mac com um atalho que ele não registra.
- [x] Segunda janela `quick`, feita na primeira vez e reaproveitada: sem moldura, sempre no topo, fora da barra de
  tarefas, na largura do monitor onde está o ponteiro e 42% da altura útil, pendurada no topo da área útil (as
  medidas de `QuickTerminal.swift`, testadas em Rust com monitores de coordenada negativa e ponteiro entre
  monitores). Desliza de cima em 0,18 s (ease-out) e sai em 0,15 s (ease-in), as curvas do Core Animation, e um
  deslize novo vira de onde a janela está. Some ao perder o foco que teve; fechá-la (Alt+F4) só a esconde, para
  reaproveitá-la (isso não foi exercitado: no Mac de desenvolvimento nada fecha uma janela sem moldura). Esc é do
  programa, como no Mac. A primeira vez espera a página desenhar (até 3 s) para não entrar em branco.
- [~] No Windows: a sombra do sistema como borda (`shadow`), sem o arredondamento nem o filete do DWM
  (`DWMWA_WINDOW_CORNER_PREFERENCE`, `DWMWA_BORDER_COLOR`), e a janela vai para a área de trabalho virtual em uso
  (`IVirtualDesktopManager`, o `canJoinAllSpaces` do Mac). A janela sem moldura com sombra é, no Windows, maior que
  o painel (a borda invisível do quadro: 8 px nos lados e embaixo, 1 em cima no Windows 11, a escala do monitor), então
  o que se posiciona é a parte desenhada (`Margins`, medida na própria janela pela diferença entre a posição interna
  e a externa) e o roteiro compara essa parte com o lugar pedido. Só compilado. O foco é pedido de novo até quatro vezes
  se o sistema demora a dá-lo (o Windows só dá o teclado a quem recebeu a última entrada).
- [~] Os cantos de baixo arredondados em 10 px e o fundo translúcido do painel do Mac (fatia `glass`). A janela
  rápida é sempre transparente no Windows (`glass::made_transparent_for_corners`; no Mac `transparent` pede API
  privada, e lá o painel segue como estava) e a página redonda só os dois cantos de baixo
  (`html.quick-round .quick-shell`, `styles-glass.css`), o de cima rente à tela como o painel do Mac. Escolha:
  o DWM do Windows 11 só arredonda os quatro cantos (`DWMWCP_ROUND`), e os de cima deixariam um vão na borda da
  tela; esconder esses dois pondo a janela para fora da tela mudaria a geometria que a fatia `quick` provou no CI
  (largura do monitor, 42% da altura, deslize de 0,18 s); então o arredondamento do sistema continua desligado
  (`soften_the_frame`) e vale o CSS no 10 e no 11, igual. Preço: a sombra é a do retângulo da janela e deixa um
  quadradinho nos cantos redondos. O fundo translúcido do painel segue a opção da janela principal (mesmo alfa, mesmo
  material, lidos do estado a cada entrada); desligada, o painel é opaco, só com os cantos. Compilado no Windows pelo
  CI e pelo roteiro (`e2eGlass.ts` confere que a janela rápida é transparente; o passo de geometria da fatia
  `quick` continua exigindo o painel no lugar certo); a aparência dos cantos e da sombra não foi vista.
- [x] A página `?quick=1` (a mesma página, `main.tsx`) mostra só o primeiro terminal vivo do workspace, sem barra
  lateral nem fileira de abas, nas margens do painel do Mac (8 nos lados, 6 em cima, 8 embaixo). Se o shell
  acaba, abre outra aba, inclusive quando o keepd devolve o número da que morreu. Tema, fonte, zoom e cursor são os
  da janela principal, lidos do `state.json` a cada entrada por `quick_look`, que só lê (não faz as recuperações,
  os avisos nem o travamento do `load_state`) e que nada grava: o estado é da janela principal. Teclado (a tecla
  que não é da janela é de `terminalKey`: Kitty, Ctrl+C numa seleção, Ctrl+X, Shift+Enter), menu de contexto,
  arquivos soltos e a pergunta da colagem de várias linhas são os de uma aba. Vistos no painel: as teclas da
  janela engolidas sem deixar caractere no shell, Ctrl+Shift+L, o menu de contexto (Limpar a tela, e o teclado
  volta ao terminal). Não exercitados nele: copiar e colar (a área de transferência é da pessoa), soltar
  arquivos e a pergunta da colagem de várias linhas; são o código das abas, ligado à página do painel.
- [x] "Janela rápida" na paleta, com o atalho em uso desenhado, ligada por `registerCommand` (`quick/main.ts`).
- [x] Ciclo de vida: a janela é do mesmo processo (o single-instance não vê um segundo app); fechar a janela
  principal sai do app, como no Mac, e leva a rápida junto; sem isso a rápida escondida contaria ao Tauri como
  janela aberta e deixaria um processo sem nada à vista (`on_run_event`); sair deixa o keepd.
- [ ] No Mac, o `work_area` do tao não desconta a barra de menus, e o painel de desenvolvimento a cobre; no
  Windows é o `rcWork`, que desconta a barra de tarefas.
- Para quem for ver no Windows: o `app.log` (`%LOCALAPPDATA%\Keep\app.log`) diz, com `janela rápida:`, se o atalho
  foi registrado, quanto a página levou a ficar pronta e se a janela recebeu o foco ao entrar; com a variável
  `KEEP_QUICK_TRACE=1` ele também registra cada passo do deslize e cada ganho e perda de foco. O VS Code usa
  Ctrl+` para o terminal dele, e o atalho global o toma enquanto o Keep estiver aberto: quem usa os dois escolhe
  outro nas Configurações.

## Fatia `windows` (várias janelas)

Cada janela é uma página (um webview) com o seu próprio estado, como cada `MainWindowController` do Mac sobre a
mesma `Session`; o que as páginas têm de combinar vive uma vez só no app (`src-tauri/src/windows.rs`), e o modelo
puro (quem lista o quê, guardar, ir para, as órfãs, a marca, o arrasto) está em `src/windowModel.ts` e
`src/windowState.ts`, com testes. Com uma janela só nada mudou: o roteiro do app (`src/e2e.ts`, 51 passos contra
um keepd de verdade) passa igual, e `state.json` de uma janela só é lido sem perder nada.

Conferido no Mac pelo roteiro (`src/e2eWindows.ts`: a janela que o próprio roteiro abre responde a ele por eventos)
e à mão (abrir, ir para, fechar, sair e reabrir com duas janelas). O que só o Windows real mostra está marcado.

- [x] Nova janela: Ctrl+Shift+Alt+N (o ⇧⌘N do Mac, cuja tecla no Windows já é "Novo workspace", com o Alt que a
  paleta também leva), "Nova janela" na paleta e no menu da fileira de abas. Nasce vazia, do tamanho da de origem,
  24 px abaixo e ao lado (maximizada se a origem estiver), com a barra lateral (largura, abas verticais) da primeira
  janela. O que o app pôs é devolvido a quem pediu (o roteiro confere): um gerenciador de janelas que maximiza toda
  janela nova (o do Mac de desenvolvimento) fala depois disso.
- [x] Lista de workspaces por janela: a ordem é da janela (`order`, que também lembra o lugar do que foi guardado);
  os guardados (`hidden`) são de todas, como o put-away.json do Mac. A janela mais antiga toma o que ninguém lista
  (criado pela linha de comando, ou enquanto o app não rodava: `placeOrphans`). Ir para adota o workspace na janela
  (no fim da lista, ou onde ela o lembrava) e o tira dos guardados; "Remover da barra lateral" (e "Remover este
  workspace da janela") só tira da lista desta janela se outra o lista, e guarda de verdade se for a última.
- [x] O que é do app e o que é da janela: de todas, os nomes dados, a ordem das abas, a última aba de cada
  workspace, os guardados, o daemon, o zoom, a aparência, o tema, a fonte e o cursor; de cada janela, a lista, o que
  está recolhido, as abas verticais, a barra lateral e a aba em frente. Uma janela diz tudo ao app numa mensagem
  (`window_sync`: o que mudou no que é de todos, o que lista e mostra, a ficha própria) e o app diz a todas numa
  (`keep://sync`, na ordem em que as mudanças foram feitas, inclusive a de quem as fez): um workspace guardado pela
  última janela que o listava sai da lista dela e entra nos guardados de uma vez, e a janela mais antiga nunca o vê
  como "de ninguém". A lista das janelas leva uma versão que só cresce.
- [x] `state.json` (versão 2): `shared`, mais `windows.<rótulo>` (`main`, `w1`, `w2`…, o mesmo espaço que o Mac chama
  de slot e reaproveita: a janela nova herda a ficha da que fechou). Tudo é lido, mudado e gravado de uma vez no
  Rust, sob uma trava, com as cópias rotativas, a recuperação e o aviso da fatia `ai`. O formato de antes (um objeto
  só) vira o `shared` e a ficha de `main`. A janela rápida lê o estado em uma peça só (`windows::look`) e continua
  sem gravar nada.
- [x] A marca da aba que outra janela mostra: duas janelas sobrepostas depois do título, na fileira e na barra
  lateral (o `macwindow.on.rectangle` do Mac), com o texto "Aberta em outra janela".
- [x] A mesma aba em duas janelas são dois visualizadores no keepd (o roteiro vê `clients` = 2). O terminal fora de
  vista numa janela diz ao keepd 0×0 ("não estou olhando") só quando há mais de uma janela: com uma, ele segue
  dizendo o seu tamanho, como sempre fez. A aba adormecida (hibernação) larga a ligação, como antes.
- [~] Arrancar a aba da fileira: puxar 40 px para cima ou para baixo da faixa dela pendura uma cópia no ponteiro e
  fecha o vão; solta, abre uma janela do tamanho da origem com a aba debaixo do ponteiro (o mesmo lugar da fileira)
  ou a entrega à janela que está debaixo dele (a mais à frente das que o contêm: `window_under`, por geometria e
  pela ordem em que as janelas estiveram à frente); a janela de origem segue para a aba vizinha se era a que a
  mostrava, e a aba continua no workspace. Conferido no Mac com os eventos de ponteiro da própria página e testado
  por testes (geometria, janela sob um ponto); o ponteiro de verdade fora da janela no WebView2 e a ordem real das
  janelas no Windows não foram vistos.
- [x] Fechar uma janela com outras abertas põe de lado (guardados) o que só ela listava, sem passar para outra
  janela (`Session.removeWindow` do Mac: o pedido da fatia dizia que passa para a primeira, mas o código do Mac
  guarda; seguimos o código) e esquece a ficha dela. Fechar a última é sair: não guarda nada e a ficha fica. Sair
  com várias abertas também não guarda nada (o app sabe que está saindo).
- [~] Abrir as janelas da última vez: a janela mais antiga as abre depois de o daemon ter as abas e de saber o que
  foi das antigas (o número da aba em frente de cada uma é levado ao novo, ou esquecido), com a lista, a aba e o
  quadro (posição, tamanho, maximizada), e só em monitor que existe (senão o sistema escolhe o lugar). Visto no Mac
  reabrindo o app com duas janelas; o quadro de uma janela maximizada e o monitor que some só no Windows real.
- [x] As rodadas do núcleo que mudam alguma coisa (restauro, `continuar`, os nomes, `sincronizar`, índice de
  worktrees, renovar logins) são da janela mais antiga; as que só leem (uso, abas) rodam em cada janela.
- [x] Quando uma janela cria, fecha ou move uma aba, as outras perguntam ao daemon na hora (`keep://refresh`) em
  vez de esperar a rodada do segundo.
- [x] Capabilities: `windows: ["main", "w*"]` (o Tauri aceita padrão); as janelas se criam em código a partir da
  configuração da primeira (sem moldura no Windows, a barra de título do Mac), com o filtro do plugin de estado de
  janela só para `main`.
- [ ] Conhecido e não resolvido: um workspace feito numa janela que não é a mais antiga (Ctrl+Shift+N, "abrir em") pode
  ser tomado também pela mais antiga se ela perguntar ao daemon antes de a janela que o fez dizer ao app que o lista
  (um pulso de 200 ms): ele aparece nas duas. Um workspace feito antes de listá-lo (`createWorkspace` pondo o nome na
  lista e avisando o app antes de pedir a aba ao keepd) foi tentado e tirado: mexia na ordem dos passos de uma janela
  só e o roteiro falhou em passos antigos que não consegui explicar a tempo. Com uma janela só o risco não existe.
- [ ] Não testado: o Windows real (nada disto rodou lá; o CI do ramo ainda não viu estas mudanças), o ponteiro de verdade
  fora da janela no WebView2, vários monitores, a janela mais antiga fechando e passando a vez (só raciocinado e coberto
  pelos testes do Rust), a janela rápida com várias janelas abertas. No Mac de desenvolvimento um gerenciador de janelas
  maximiza toda janela nova, e o roteiro das janelas só olha onde o app as pôs. Passos de outras fatias (janela rápida:
  atalho global) falharam de vez em quando neste Mac com outras sessões usando a tela e o atalho.
- [ ] Tela cheia por janela no estado: o Mac também só guarda o quadro; a janela volta no quadro, não em tela cheia.
- [~] Janela rápida global (Ctrl+`), com o workspace `quick` oculto: feita na fatia `quick`, acima.
