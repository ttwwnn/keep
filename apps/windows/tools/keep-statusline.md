# Barra do Claude Code no Keep para Windows

A barra sob a conversa mostra o modelo, o esforço e os agentes do Claude Code. O Claude Code
informa o modelo e o esforço apenas ao comando de `statusLine`; este script grava o que recebe
para o Keep ler. O Keep não altera o seu `settings.json`: a ligação abaixo é manual.

## Ligar

1. Copie `keep-statusline.ps1` (esta pasta) para `%USERPROFILE%\.local\bin\keep-statusline.ps1`.
   No PowerShell, a partir da pasta do repositório:

   ```powershell
   New-Item -ItemType Directory -Force "$env:USERPROFILE\.local\bin" | Out-Null
   Copy-Item apps\windows\tools\keep-statusline.ps1 "$env:USERPROFILE\.local\bin\"
   ```

2. Em `%USERPROFILE%\.claude\settings.json`, acrescente (troque `SEU_USUARIO`; barras normais,
   porque o Claude Code pode executar o comando por um shell que não entende `\`):

   ```json
   {
     "statusLine": {
       "type": "command",
       "command": "powershell -NoProfile -ExecutionPolicy Bypass -File C:/Users/SEU_USUARIO/.local/bin/keep-statusline.ps1"
     }
   }
   ```

   O script não imprime nada: a linha de status do próprio Claude Code fica vazia, porque o Keep
   mostra essas informações na barra. Se você já usa uma `statusLine`, ela será substituída.

3. Abra uma aba nova no Keep e rode `claude`. Em alguns segundos a barra aparece sob a conversa.

## Conferir

Numa aba do Keep com o Claude Code aberto, em outro terminal:

```powershell
Get-ChildItem "$env:USERPROFILE\.local\state\keep\claude"
```

Deve haver um `<número>.json`, o número do shell da aba. Sem arquivo, a barra não aparece.

## Como funciona

- O script lê o JSON que o Claude Code envia, procura o processo de shell da aba (o ancestral
  cujo pai é o `keepd.exe`) e grava o JSON em
  `%USERPROFILE%\.local\state\keep\claude\<pid do shell>.json`.
- Fora de uma aba do Keep não há `keepd.exe` entre os ancestrais, e o script não grava nada.
- `KEEP_STATUS_DIR` troca a pasta, para o script e para o app.
- Os agentes são contados na tela do Claude Code (as linhas `◯` sob o prompt), sem este script.

## Codex

Não precisa de nada. O Codex não tem comando de `statusLine`: o Keep lê o modelo e o esforço do
rodapé da própria tela do Codex (por exemplo `gpt-6.1-sol high · 100% context left`).

## Trocar pela barra

Clicar no modelo ou no esforço digita o comando na conversa, só com o turno terminado e o prompt
vazio (senão o menu fica desligado, com um aviso):

- Claude Code: `/model <id>` e `/effort <nível>` (`low`, `medium`, `high`, `xhigh`, `max`).
- Codex: `/model`, que abre o seletor do próprio Codex; escolha ali com as setas e Enter.
