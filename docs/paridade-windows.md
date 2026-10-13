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
- [ ] Endereços web: clique simples abre no navegador, inclusive o link OSC 8
  atrás do texto (markdown do Claude Code) e o endereço quebrado em linhas;
  menu Abrir link / Copiar link (Mac: 0e8c0bc).
- [ ] OSC 52 com política; confirmar colagem multi-linha; colar arquivo copiado.
- [ ] Soltar arquivos: realce do painel sob o ponteiro.
- [ ] Teclado kitty (CSI u): Ctrl+Enter, desambiguação.
- [ ] Cut multi-linha no prompt do Claude.
- [ ] Título OSC imediato (onTitleChange); grafemas Unicode (modo 2027).

## Fatia `ai`

- [~] Integração de shell (função `claude` e `codex` no PowerShell das abas): troca a quente, armazém por
  aba. Feita pelo keepd (`crates/keepd/src/ai.ps1`, `shellhook.rs`); testada com um `pwsh` 7 de verdade e
  programas de mentira (argumentos, pipe, código de saída, falha segura, subcomandos, função da pessoa),
  e conferida para o Windows com `cargo check --target x86_64-pc-windows-msvc`; não rodou num Windows nem
  no PowerShell 5.1. Cmd e Git Bash ficam de fora (rodam no login global) e o WSL é invisível ao crate
  (docs/ia.md).
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

- [ ] Restringir a IA do workspace a uma pasta no Windows (AppContainer ou
  token restrito), botão de pasta, menu "Restringir a IA a uma pasta…" e
  "Liberar a IA". Só vale para workspaces com pasta escolhida (opt-in).

## Fatia `windows` (várias janelas)

- [ ] Nova janela; lista de workspaces, aba e painel por janela; mover
  workspace entre janelas; arrancar a aba da faixa.
- [ ] Janela rápida global (Ctrl+`), com o workspace `quick` oculto.
- [ ] Estado das janelas (quadro, tela cheia).
