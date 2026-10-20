## Keep para Windows e macOS

**Windows (x64):** baixe `Keep_<versão>_x64-setup.exe` e rode. Instala só para o seu usuário,
sem administrador, com o daemon (`keepd`), o cliente de linha de comando (`keep`, em
`%LOCALAPPDATA%\Keep\bin`) e o console do Windows Terminal (ConPTY da Microsoft).
O instalador não é assinado: o Windows pode mostrar "O Windows protegeu o computador" —
clique em **Mais informações** e **Executar assim mesmo**.

**macOS (Apple Silicon):** `Keep-macOS-arm64.zip`, assinado ad hoc. Depois de descompactar e
mover para Aplicativos: `xattr -dr com.apple.quarantine /Applications/Keep.app`. O kit-mac instala
esta release num Mac sem Xcode (e a assina com a identidade local, para o Acesso Total ao Disco
sobreviver às atualizações); com o Xcode, compila o ramo `main`.

Fechar a janela não fecha nada: os terminais continuam no daemon e voltam como estavam.

**IA no Keep, nos dois sistemas, sem nada além do próprio Keep:**
- rodapé **Consumo de IA**: cada conta do Claude e do GPT na ordem de prioridade, com as janelas de
  5 h e da semana; setas para mudar a ordem, "+" para entrar em outra conta, ↻ para medir agora;
- **Jev**: o rodapé mostra o crédito do OpenRouter que o Jev gasta (o que resta da conta e o que a
  chave dele já gastou, ou o que resta do teto dela, se tiver um), e o "+" traz "Conectar o Jev ao
  OpenRouter…", que abre o login do OpenRouter no navegador e guarda a chave (no Chaveiro do macOS,
  no Gerenciador de Credenciais do Windows);
- **IA de cada aba**: o menu ao lado do título troca a conta (a mesma conversa continua) ou o motor
  (Claude ↔ Codex, com o contexto da conversa), e "Seguir a ordem de prioridade" leva a aba para a
  primeira conta do Claude disponível quando a dela chega ao limite (o GPT só entra escolhido no menu);
- **login que não cai**: o Keep renova sozinho, antes de vencer, o login de cada conta do Claude,
  inclusive o da conta parada, e trocar de conta nunca pede login de novo; a aba aberta para um
  login novo fecha sozinha quando ele termina;
- **lixeira de worktrees**: ao fechar uma aba, as worktrees git que a conversa dela criou vão para a
  Lixeira, depois de você confirmar.

**Marcas e cores das abas (0.2.3):**
- o `✳` de uma aba em trabalho é laranja para o Claude Code e um `◆` azul-esverdeado para o Codex (no Mac,
  na barra lateral; no Windows, na faixa e na barra); qualquer outro comando mantém o `✳` amarelo;
- uma aba cujo Claude Code tem um processo filho na frente (servidor MCP, `php`) e que um daemon antigo
  nomeia como shell volta a ser reconhecida e a ter cor.

**Crédito do Jev em tempo real (0.2.4):**
- o rodapé soma na hora o custo de cada decisão do Jev feita nesta máquina, sem esperar o OpenRouter,
  que leva uns dois minutos para contá-la: a skill `jev` anota cada chamada em
  `~/.claude/jev/chamadas.jsonl` (quando, quanto custou e um resumo da chave, nunca a chave), e cada
  leitura nova desconta o que o OpenRouter já contou;
- o crédito do OpenRouter é lido a cada minuto, em vez de a cada cinco.

**Gasto da conta (0.2.5):**
- com a chave do Jev sem teto, o segundo contador do Jev, "gasto", é o que a conta toda já gastou, de
  todas as chaves, e não só a do Jev: "restam" e "gasto" somam o crédito comprado; o gasto da chave do
  Jev fica na dica.

**Abas guardadas, zoom e novidades (0.2.6):**
- **hibernação sem perda:** a aba hibernada é guardada pelo próprio cliente `keep` (espelho da tela e
  entrega do terminal ao daemon), e nada do que ela tinha se perde ao acordar: bordas, links após
  quebra de linha, área de transferência, cor do tema e imagem desenhada (que acorda a aba);
- **reinício traz tudo de volta:** `keep restauro` recria as abas e as conversas sozinho, e
  `keep continuar` retoma a conversa parada por login caído, rede ou limite de uso;
- **troca de conta a quente:** a conversa do Claude muda de conta sem fechar, no armazém de login da
  própria aba, e o título mostra "(IA·1→2)" até o Claude responder na conta nova;
- **contas pela posição:** cada conta é "IA-n", a ordem do rodapé de consumo, em todas as telas;
- **barra do Claude Code:** modelo e esforço clicáveis só para a conversa, e ajudantes
  (`keep nova`, `keep ia statusline`, `confiar`, `confinar`) dentro do próprio Keep;
- **zoom:** o 100% é o tamanho da fonte do usuário mais 25%; o texto do chrome, os ícones, a bateria
  e o quadro de memória acompanham o zoom. O quadro de RAM abre o Monitor de Atividade;
- **lixeira de worktrees:** as worktrees de abas fechadas sem a pergunta também vão para a Lixeira
  (macOS e Windows); o que o Keep mandou para a Lixeira sai de vez depois de 3 dias;
- ⌘X recorta pelas linhas da caixa do Claude Code, ⌘V entra "bracketed" quando o programa pede, e o
  Windows entende "max" e "ultra" do Codex 0.162.

**Paridade do Windows com o Mac (0.3.0):** o Windows passa a ter o que o Mac tem, com a mesma aparência.
- **aparência:** textos, ícones, barra lateral, fileira de abas, zoom, bateria e quadro de RAM como no Mac;
  cursor em barra; 805 temas do Ghostty com prévia ao vivo e lista de fontes monoespaçadas; título da
  janela igual ao da aba; o terminal avisa o programa de claro ou escuro (modo 2031);
- **seletor e paleta:** Ir para com prévia colorida, painel de ações (Ctrl+K), paleta de comandos
  (Ctrl+Shift+Alt+P) e busca no painel e em tudo, com salto à linha exata;
- **abas e painéis:** reordenar abas por arrasto, mover para outro workspace, abas verticais, alça e
  divisor de painel, foco por Alt+setas, hibernação das abas fora de vista e workspaces guardados;
- **terminal:** caminhos de arquivo clicáveis com menu (Abrir, Mostrar no Explorer, Copiar), links web
  com clique, OSC 52, confirmação ao colar várias linhas, teclado Kitty, grafemas Unicode (modo 2027),
  colar arquivo copiado no Explorer e resposta automática a "confiar nesta pasta?";
- **IA:** funções `claude` e `codex` no PowerShell, reinício com as conversas de volta, `keep continuar`,
  ganchos da conta da aba e cópias de segurança do estado;
- **confinamento:** a IA de um workspace pode ser presa a uma pasta (token restrito do Windows); a
  ferramenta Bash do Claude Code usa o PowerShell enquanto presa;
- **janelas:** várias janelas com lista de workspaces própria, arrancar a aba da fileira, e a janela
  rápida global (Ctrl+`, configurável);
- **opcionais, desligados por padrão:** fundo translúcido com Mica ou Acrílico (pede reinício do app) e
  links `claude-cli://` abrindo uma aba no Keep (Configurações, "Links do Claude Code").

No Mac: cursor em barra, ícones e textos 25% maiores, "(IA·n)" e o quadro de RAM no tamanho certo.

**Botões do Jev e do Ponytail (0.3.1):** embaixo do "Consumo de IA", um botão de liga e desliga para cada um,
no Mac e no Windows (`keep ia automacao jev|ponytail ligar|desligar`).
- **Jev:** desligado, os ganchos da skill `jev` não fazem nada (`~/.claude/jev/desligado`); ligar apaga o arquivo;
- **Ponytail:** o plugin `ponytail@ponytail` ligado ou desligado no Claude Code (`claude plugin enable|disable`) e
  no Codex (`[plugins."ponytail@ponytail"]` do `config.toml`), os dois juntos; o botão mostra o que o Claude tem;
- o que não está instalado não ganha botão; um botão mudado no terminal aparece em segundos, e uma recusa
  aparece no rodapé, com o botão de volta ao que a IA tem.

Detalhes em `docs/ia.md`.
