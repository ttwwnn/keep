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
- [~] Encaixe na função `claude` do PowerShell (fatia `ai`): trocar o
  `& $claude @args` final por `& $keep ia confinar -- $claude @args`; sem pasta
  para o workspace o `confinar` roda o programa como sempre. Enquanto a função
  não passa por ele, só a troca de conta/IA e a restauração (que já chamam
  `envolve`) prendem; digitar `claude` no shell, não.
- [ ] O que o Windows não deixa fechar sem administrador: pastas de rede (o
  servidor confere o usuário, não o token), o que o Windows deixa qualquer um
  ler (`C:\Windows`, `Arquivos de Programas`, e, nos níveis compatível e amplo,
  que valem se o estrito não deixa o programa partir, o que `Usuários` lê em
  outras unidades), o registro do sistema, a área de trabalho (o programa preso
  pode mandar teclas a outras janelas), o WSL. Programas
  instalados só para o usuário (nvm, cargo, pyenv) ficam de fora, como o
  `~/.nvm` no Mac; o Gerenciador de Credenciais do Git também.

## Fatia `windows` (várias janelas)

- [ ] Nova janela; lista de workspaces, aba e painel por janela; mover
  workspace entre janelas; arrancar a aba da faixa.
- [ ] Janela rápida global (Ctrl+`), com o workspace `quick` oculto.
- [ ] Estado das janelas (quadro, tela cheia).
