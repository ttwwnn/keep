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

**Barra do Claude e gancho da conta da aba em qualquer Mac (0.3.2):** ao abrir, o app do Mac grava no
`~/.claude/settings.json` a statusLine do Keep (`keep ia statusline`, de onde a barra tira o modelo, o esforço
e os agentes de cada conversa) e o gancho `keep ia conta-da-aba --gancho` (o Claude de uma aba trocada a quente
diz a conta certa), como o Windows já fazia (`keep ia ajustar-claude`). Antes eles só existiam à mão no Mac do
autor, e um Mac novo ficava sem a barra.
- o resto do arquivo fica como estava, byte a byte, e a permissão também; uma statusLine ou um gancho que já
  existam de outro lugar não são tocados; sem a pasta do Claude Code nada é criado.

**Tema do Keep em qualquer Mac (0.3.2):** o app escreve os temas "Keep Escuro" (o escuro padrão do Ghostty) e
"Keep Claro" (fundo branco, para o tema claro do Claude Code) em `~/.config/ghostty/themes` quando faltam, nunca
por cima de um arquivo com o mesmo nome, e os usa como padrão (`dark:Keep Escuro,light:Keep Claro`, seguindo o
claro e o escuro do macOS) quando nem a configuração do Ghostty nem o Keep escolheram um tema. Antes só o kit-mac
os instalava.

**Login do Claude sem queda com o kit pela metade (0.3.3):** o Keep só deixa a renovação do login global para o
kit-mac quando o agente de renovação dele (`br.ttwwnn.kit.claude-conectado`) está ligado. Antes bastavam o cofre e
os scripts do kit; num Mac com o kit instalado pela metade ninguém renovava, e o Claude caía na tela de login.

Detalhes em `docs/ia.md`.

**Modelo e esforço na conversa nova (0.3.4):** na barra sob a conversa do Claude, os itens de modelo e esforço
ficavam desativados numa conversa nova, porque a sugestão cinza da caixa vazia (`❯ Try "…"`) parecia texto
digitado. Agora o texto esmaecido não conta, e a barra troca o modelo e o esforço assim que a caixa está vazia.

**Imagem colada abre no clique (0.3.5, Mac):** um clique no `[Image #N]` de uma imagem colada no Claude Code
abre a imagem no app padrão (em geral, o Visualização). Com o Claude Code em tela cheia, o clique ia para ele,
que só mostrava o arquivo no Finder; agora o Keep o retém quando há uma imagem por trás do texto. Duplo clique
não abre nada, e arrastar continua selecionando.

**Imagem colada abre também na caixa de texto (0.3.6, Mac):** antes do envio, o Claude Code ainda não gravou a
imagem nem pôs link no `[Image #N]`. O Keep guarda uma cópia da área de transferência ao colar (⌘V só com
imagem, ⌘⇧V ou Ctrl+V) e associa a ela o número que aparece; o clique no `[Image #N]` da caixa abre essa cópia.

**Ícone de robô (0.3.7):** o Keep ganhou ícone próprio, um robô com um terminal por rosto, no Mac (antes aparecia o
ícone genérico de app) e no Windows.

**Conversa parada há 12 h sai da memória (0.3.8):** a aba do Claude fora de vista, livre e sem fala há mais de 12 h
fecha o Claude Code (cada um pesa de 200 a 400 MB) e fica no Keep com o shell; a conversa volta sozinha, com as
mesmas opções e a mesma conta, quando a aba é mostrada. Depois de um reinício ela volta assim também. Mac e Windows.

**Aba adormecida com o nome da conversa (0.3.9):** a aba cuja conversa saiu da memória fica com o título da conversa
como nome na barra (se a pessoa não lhe deu outro), e o nome sai quando ela acorda.

**0.3.10:** o app do Mac da 0.3.8 e da 0.3.9 não compilou no CI (as duas releases saíram só com o instalador do
Windows); esta traz as duas mudanças acima no Mac também.

**Pergunta de confiança respondida ao retomar (0.3.11):** a conversa que volta (depois de um reinício, ou ao
acordar) numa pasta que o Claude Code ainda não conhece não fica mais parada na pergunta "confiar nesta pasta?":
o Keep responde sim por ela, como já fazia ao abrir uma conversa nova.

**Qualquer arquivo abre no clique (0.3.12, Mac):** com o Claude Code em tela cheia, só a imagem era retida pelo Keep;
o clique em link de outro arquivo (PDF, planilha, texto, pasta) ia ao Claude Code, que só o mostrava no Finder.
Agora o Keep retém o clique em qualquer arquivo que exista atrás do link e o abre no app padrão. Duplo clique
não abre nada, e arrastar continua selecionando.

**Áudio abre no clique (0.3.13, Mac):** o Music, app padrão de mp3, abria e fechava na hora sem tocar nada. O Keep
passa a abrir áudio no QuickTime Player quando o padrão do Mac para ele é o Music; se você escolher outro app para
áudio, o Keep respeita.

**Nome solto de arquivo abre no clique (0.3.14, Mac):** quando o Claude cita só o nome de um arquivo que ele gravou na
pasta temporária da sessão (`pergunta-nova.mp3`), o nome não existe na pasta da aba e o clique não achava nada. Agora
o Keep procura o nome em `/private/tmp/claude-<uid>/*/*/scratchpad` (e uma pasta abaixo) e abre o mais recente.

**O Keep aberto pelo Dock não fecha mais sozinho (0.3.15, Mac):** aberto pelo Dock, pelo Finder ou no login (sem `LANG`
no ambiente), o app fechava meio segundo depois de abrir, toda vez, e só voltava aberto de um terminal. Ele gravava uma
variável no próprio ambiente depois de iniciar o libghostty, que guarda esse ambiente e passava a ler memória liberada.
Agora a variável vai só para os comandos do núcleo e o app não mexe mais no próprio ambiente.

**Hibernar a aba na hora, com o zzz (0.3.16, Mac e Windows):** ao lado do × da linha de cada aba com o Claude
aparece, sob o ponteiro, um **zzz**. Ele tira a conversa da memória na hora, como as 12 horas paradas fariam: a aba
segue na barra lateral e na faixa de abas com o título riscado, e a conversa volta quando você abre a aba. O zzz não
aparece enquanto a aba trabalha ou espera você, e, se o Keep recusar, a janela diz por quê (um comando rodando, uma
pergunta aberta, texto na caixa). No Windows a hibernação das 12 horas passa a funcionar também: o app recusava o
pedido ao núcleo, e ela nunca tinha rodado lá. E o Keep passa a enxergar como Claude as abas em que o keepd diz
`zsh` com o Claude na frente (as mais antigas, de dias): elas ficavam fora da hibernação, do menu de conta e do zzz.

**A pergunta de fechar cabe na tela (0.3.17, Mac):** a pergunta antes de fechar uma aba cuja conversa tinha feito
muitas worktrees crescia além da tela: com 27, os botões ficavam abaixo da borda e nenhuma tecla respondia, e o Keep
não fechava nem cancelava. Agora a lista vai numa caixa que rola a partir de metade da tela, como no Windows, os botões
ficam sempre à vista e o Esc cancela. O Return segue sem fechar a aba, porque o botão de fechar é destrutivo. O mesmo
vale para o aviso do que não foi para a lixeira e para a pergunta antes de colar várias linhas.

**Workspace novo não some mais (0.3.18, Mac e Windows):** criar um workspace às vezes não dava em nada: a aba nascia,
mas o workspace saía da lista e o shell ficava rodando sem que ninguém o visse. Acontecia quando o app pedia a lista de
abas no instante em que a primeira aba abria, o que no Windows, onde o PowerShell demora mais a abrir, era frequente.
Agora o workspace fica de pé até a primeira aba entrar. E o daemon passa a anotar em `abas.log` a aba que morre nos
primeiros segundos sem ninguém olhando, com o código de saída do shell.
