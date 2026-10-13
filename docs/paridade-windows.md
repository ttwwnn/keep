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
  menu Abrir link / Copiar link (Mac: 0e8c0bc).
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

- [ ] Restringir a IA do workspace a uma pasta no Windows (AppContainer ou
  token restrito), botão de pasta, menu "Restringir a IA a uma pasta…" e
  "Liberar a IA". Só vale para workspaces com pasta escolhida (opt-in).

## Fatia `windows` (várias janelas)

- [ ] Nova janela; lista de workspaces, aba e painel por janela; mover
  workspace entre janelas; arrancar a aba da faixa.
- [ ] Janela rápida global (Ctrl+`), com o workspace `quick` oculto.
- [ ] Estado das janelas (quadro, tela cheia).
