## Keep para Windows e macOS

**Windows (x64):** baixe `Keep_<versão>_x64-setup.exe` e rode. Instala só para o seu usuário,
sem administrador, com o daemon (`keepd`), o cliente de linha de comando (`keep`, em
`%LOCALAPPDATA%\Keep\bin`) e o console do Windows Terminal (ConPTY da Microsoft).
O instalador não é assinado: o Windows pode mostrar "O Windows protegeu o computador" —
clique em **Mais informações** e **Executar assim mesmo**.

**macOS (Apple Silicon):** `Keep-macOS-arm64.zip`, assinado ad hoc. Depois de descompactar e
mover para Aplicativos: `xattr -dr com.apple.quarantine /Applications/Keep.app`. No Mac do autor
o app é instalado pelo kit, compilado do ramo `main` e assinado com a identidade local.

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

Detalhes em `docs/ia.md`.
