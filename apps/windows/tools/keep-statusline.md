# Barra do Claude Code e do Codex no Keep para Windows

A barra sob a conversa mostra o modelo, o esforço e os agentes do Claude Code, e o modelo e o
esforço do Codex. O Claude Code informa o modelo e o esforço apenas ao comando de `statusLine`;
este script (`keep-statusline.ps1`) grava o que recebe para o Keep ler.

## Ligação automática

O instalador põe o script ao lado do app (`%LOCALAPPDATA%\Keep\keep-statusline.ps1`). Ao abrir, o
Keep confere o `settings.json` do Claude Code (`%USERPROFILE%\.claude\settings.json`, ou o da pasta
de `CLAUDE_CONFIG_DIR`):

- sem `statusLine`: acrescenta a chave apontando para o script, antes da chave de fechamento, no
  recuo e nas quebras de linha do próprio arquivo, sem mexer em mais nada;
- já com a `statusLine` do Keep: nada a fazer;
- com outra `statusLine`: não toca e registra no log do app (`%LOCALAPPDATA%\Keep\app.log`);
- sem a pasta do Claude Code (ele ainda não rodou nesta máquina): não cria nada; a ligação
  acontece na primeira abertura depois disso.

A chave gravada é esta (o caminho em barras normais e entre aspas, porque o Claude Code executa
o comando pelo Git Bash, ou pelo cmd na falta dele):

```json
{
  "statusLine": {
    "type": "command",
    "command": "powershell -NoProfile -ExecutionPolicy Bypass -File \"C:/Users/SEU_USUARIO/AppData/Local/Keep/keep-statusline.ps1\""
  }
}
```

O script não imprime nada: a linha de status do próprio Claude Code fica vazia, porque o Keep
mostra essas informações na barra.

## Ligar à mão (quando já há outra statusLine)

Troque a `statusLine` do `settings.json` pela acima, com o seu usuário no caminho. A sua deixa
de rodar: o Claude Code executa uma só.

## Conferir

Numa aba do Keep com o Claude Code aberto, em outro terminal:

```powershell
Get-ChildItem "$env:USERPROFILE\.local\state\keep\claude"
```

Deve haver um `<número>.json`, o número do shell da aba. Sem arquivo, a barra não aparece. Com
`KEEP_STATUSLINE_TRACE` apontando para um arquivo, o script registra nele cada execução: os
processos percorridos e o que gravou.

## Como funciona

- O script lê o JSON que o Claude Code envia, procura o processo de shell da aba (o ancestral
  cujo pai é o `keepd.exe`) e grava o JSON em
  `%USERPROFILE%\.local\state\keep\claude\<pid do shell>.json`.
- Fora de uma aba do Keep não há `keepd.exe` entre os ancestrais, e o script não grava nada.
- `KEEP_STATUS_DIR` troca a pasta, para o script e para o app.
- Os agentes são contados na tela do Claude Code (as linhas `◯` sob o prompt), sem este script.

## Codex

Não precisa de nada. O Codex não tem comando de `statusLine`: o Keep lê o modelo e o esforço do
rodapé da própria tela do Codex (por exemplo `GPT-6-Astra max · ~/projeto`), e o cache de modelos
do Codex (`%USERPROFILE%\.codex\models_cache.json`, ou o de `CODEX_HOME`) diz o nome de cada
modelo, a ordem e os esforços que cada um aceita.

## Trocar pela barra

Clicar no modelo ou no esforço digita na conversa, só com o turno terminado e o prompt vazio
(senão o menu fica desligado, com um aviso). A troca percorre o seletor do próprio programa por
teclas, lendo a tela antes de cada uma; um passo que não aparece é desfeito com Esc.

- Claude Code, só nesta conversa: o modelo por `/model`, as setas até o item e `s` ("use this
  session only"), com a pergunta "Switch model?" respondida que sim; o esforço por `/effort`, as
  setas na régua até o nível e `s`. O Opus 5.5 (1M), que a lista não tem, vai por
  `/model claude-opus-5-5[1m]`; como esse comando grava o padrão para as sessões novas, o Keep
  devolve `model`, `effortLevel` e `modelSettings` do `settings.json` como estavam, 2 e 5
  segundos depois.
- Codex: `/model`, o número do modelo e o do nível (Max e Ultra em "More reasoning…"), com os
  modelos e os esforços do cache.
