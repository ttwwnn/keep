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
- [ ] Ações de tema e de fonte: dependem dos catálogos da fatia `appearance`
  (o gancho é `registerCatalog` em commands.ts; o componente da lista tem as
  próprias ações).
- [x] Paleta de comandos: os 17 do Mac em 3 grupos, com as teclas desenhadas e
  os destrutivos em vermelho, mais Configurações, Tela cheia e Colar imagem. O
  botão redondo da fileira de abas virou "Comandos" (a engrenagem é um comando
  e segue em Ctrl+,).
- [ ] Quatro comandos ficam cinza até a fatia dona deles ligar o comando com
  `registerCommand` (commands.ts): Alternar abas verticais (`tabs`), Nova janela
  (`windows`), Remover este workspace da janela (`tabs`) e Usar o tema do Keep
  (`appearance`). "Mudar o tema…" e "Mudar a fonte…" abrem as Configurações até
  haver catálogo registrado.
- [x] Busca no painel (Ctrl+F, pelo keepd) e em tudo (Ctrl+Shift+F): escopo,
  contador "3 de 47", prévia, ida à aba, ao painel e à linha exata (contada do
  fim, como o Mac), com a linha marcada por instantes.

## Fatia `appearance` (temas, fontes, janela)

- [ ] Catálogo de temas do Ghostty (embutido), com amostra, "usar o tema do
  Keep", ground/recessed derivados do tema.
- [ ] Catálogo de fontes monoespaçadas com amostra; fallback Nerd Font.
- [ ] Cursor que pisca (ou não) e rolagem (scrollback) configuráveis.
- [ ] Título da janela = título da aba ativa.
- [ ] Modo 2031 (avisar o programa de claro/escuro) e cores de busca legíveis
  no tema claro.
- [ ] Opacidade e desfoque de fundo (Mica/Acrylic), se o Tauri permitir.

## Fatia `tabs`

- [ ] Reordenar abas por arrasto (faixa e barra lateral); mover aba para outro
  workspace (api.moveTab).
- [ ] Alça do painel: trocar, encostar nos 4 lados, soltar numa aba, destacar
  em aba nova; mola de 0,45 s (api.rearrange).
- [ ] Divisor arrastável; foco e redimensionamento de painéis por teclado.
- [ ] Alternar abas verticais (Vertical Tabs).
- [ ] Selo cols x rows; marca de painéis na aba; tooltip do workspace.
- [ ] Renomear sem fixar o título atual; o herdeiro herda o nome ao fechar a raiz.
- [ ] Workspace padrão quando o keepd está vazio.
- [ ] Hibernação: no máximo 6 abas com superfícies, LRU.
- [ ] Workspaces guardados ("put away") e "Remover desta janela".

## Fatia `terminal`

- [ ] Caminhos de arquivo clicáveis (relativos ao cwd, ~, :linha:col, quebrados
  em duas linhas), clique abre; menu Abrir / Mostrar no Explorer / Copiar.
- [ ] URLs mailto: e file://; Ctrl+clique.
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
