# `keep ia confinar` no Windows: o programa de um workspace com pasta permitida
# só lê e grava nela (e no mínimo de que a IA precisa); sem pasta, roda como
# sempre; com a pasta sumida ou larga demais, não sobe; o que mandaria trabalho
# para um servidor de fundo é recusado. Token restrito de verdade, sem o app.
# É o `tools/confine-test.sh` do macOS, e começa pelo autoteste que o próprio
# `keep` traz (`keep ia confinar --autoteste`). Rode DENTRO de uma aba do Keep
# para conferir também que o console da aba chega ao programa preso.
#
#   powershell -NoProfile -File tools\confine-test.ps1 [-Keep C:\caminho\keep.exe]
#
# O `keep` é o de `target\release` deste repositório, o do PATH, ou o dado.
param([string]$Keep = "")

# Sem `Stop`: no Windows PowerShell 5.1 a primeira linha que o `keep` escreve no
# erro (`Keep: …`), redirecionada por `2>&1`, seria uma exceção e acabaria o
# roteiro. E quem chama (o `shell: pwsh` do GitHub Actions) põe `Stop` no escopo
# de cima, que este herdaria.
$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
if (-not $Keep) {
    $local = Join-Path $repo "target\release\keep.exe"
    if (Test-Path $local) { $Keep = $local }
    else {
        $achado = Get-Command keep -ErrorAction SilentlyContinue
        if ($achado) { $Keep = $achado.Source }
    }
}
if (-not $Keep -or -not (Test-Path $Keep)) {
    Write-Host "sem keep.exe: cargo build --release -p keep (ou -Keep C:\caminho\keep.exe)"
    exit 1
}

$passou = 0
$falhou = 0
function Confere([string]$o_que, $esperado, $obtido) {
    if ("$obtido" -eq "$esperado") {
        Write-Host "  ok    $o_que"
        $script:passou++
    } else {
        Write-Host "  FALHOU  $o_que"
        Write-Host "        esperado: $esperado"
        Write-Host "        obtido:   $obtido"
        $script:falhou++
    }
}

# Sob a casa, que é o que se nega, e fora do que a IA pode guardar.
$trabalho = Join-Path $env:USERPROFILE (".keep-confine-test-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
$permitida = Join-Path $trabalho "permitida"
$fora = Join-Path $trabalho "fora"
New-Item -ItemType Directory -Force -Path $permitida, $fora | Out-Null
Set-Content -NoNewline -Path (Join-Path $permitida "a.txt") -Value "dentro"
Set-Content -NoNewline -Path (Join-Path $fora "b.txt") -Value "fora"
$config = Join-Path $trabalho "confinement.json"
function Grava-Config($pasta) {
    $json = @{ presa = $pasta } | ConvertTo-Json -Compress
    Set-Content -Path $config -Value $json
}

try {
    Write-Host "o autoteste do próprio keep"
    & $Keep ia confinar --autoteste
    Confere "keep ia confinar --autoteste" 0 $LASTEXITCODE

    $env:KEEP_CONFINE_CONFIG = $config
    Grava-Config $permitida
    function Roda([string]$workspace, [string[]]$comando) {
        $env:KEEP_WORKSPACE = $workspace
        $saida = & $Keep ia confinar -- @comando 2>&1 | Where-Object { "$_" -notmatch '^Keep: ' }
        return (($saida | ForEach-Object { "$_" }) -join "`n").Trim()
    }

    # Roteiros de uma linha dentro da pasta permitida: o caminho com espaço
    # (o nome do usuário pode ter) não passa por aspas de argumento.
    $d = Join-Path $fora "d.txt"
    Set-Content -Path (Join-Path $permitida "grava.cmd") -Value "@echo off`r`necho novo> `"%~dp0c.txt`"`r`n"
    Set-Content -Path (Join-Path $permitida "grava-fora.cmd") -Value "@echo off`r`necho x> `"$d`"`r`n"

    Write-Host "um workspace com pasta"
    Confere "lê na pasta" "dentro" (Roda "presa" @("cmd.exe", "/d", "/c", "type", (Join-Path $permitida "a.txt")))
    Roda "presa" @((Join-Path $permitida "grava.cmd")) | Out-Null
    $nova = Join-Path $permitida "c.txt"
    Confere "grava na pasta" "novo" ((Get-Content -Raw $nova -ErrorAction SilentlyContinue) -replace "\s+$", "")
    $leu = Roda "presa" @("cmd.exe", "/d", "/c", "type", (Join-Path $fora "b.txt"))
    Confere "não lê o que está fora" "False" ($leu -match "fora")
    Roda "presa" @((Join-Path $permitida "grava-fora.cmd")) | Out-Null
    Confere "não grava fora" "False" (Test-Path $d)
    $lista = Roda "presa" @("cmd.exe", "/d", "/c", "dir", "/b", $env:USERPROFILE)
    Confere "não lista a casa" "False" ($lista -match "Documents|Documentos|Desktop|AppData")
    Confere "sabe a pasta" $permitida (Roda "presa" @("cmd.exe", "/d", "/c", "echo %KEEP_CONFINED_TO%"))
    $env:KEEP_WORKSPACE = "presa"
    & $Keep ia confinar -- cmd.exe /d /c exit 7 | Out-Null
    Confere "o código de saída volta" 7 $LASTEXITCODE

    # Programas de verdade, não só o cmd: o que a IA usa no dia a dia. Cada um
    # só entra se está instalado. Quando algo falha, mostra o que o programa
    # disse e o rastro do `keep` (KEEP_CONFINE_TRACE), que é o que explica.
    Write-Host "programas de verdade"
    function Programa([string]$o_que, [string[]]$comando, [scriptblock]$criterio, [switch]$mostrar) {
        $env:KEEP_WORKSPACE = "presa"
        $env:KEEP_CONFINE_TRACE = "1"
        $saida = & $Keep ia confinar -- @comando 2>&1
        $codigo = $LASTEXITCODE
        Remove-Item Env:\KEEP_CONFINE_TRACE -ErrorAction SilentlyContinue
        $linhas = @($saida | ForEach-Object { "$_" })
        $texto = (($linhas | Where-Object { $_ -notmatch '^Keep(\[confinar\])?: ' }) -join "`n").Trim()
        if (& $criterio $codigo $texto) {
            Write-Host "  ok    $o_que"
            if ($mostrar) {
                Write-Host "        saiu com $codigo; a saída:"
                foreach ($l in $linhas) { Write-Host "          $l" }
            }
            $script:passou++
        } else {
            Write-Host "  FALHOU  $o_que"
            Write-Host "        saiu com $codigo; a saída:"
            foreach ($l in $linhas) { Write-Host "          $l" }
            $script:falhou++
        }
    }
    $git = (Get-Command git -ErrorAction SilentlyContinue).Source
    $node = (Get-Command node -ErrorAction SilentlyContinue).Source
    $bash = $null
    if ($git) {
        $achado = Join-Path (Split-Path -Parent (Split-Path -Parent $git)) "bin\bash.exe"
        if (Test-Path $achado) { $bash = $achado }
    }
    function Posix([string]$caminho) { return ($caminho -replace '\\', '/') }
    if ($git) {
        $repo = Join-Path $permitida "repo"
        Programa "git roda" @($git, "--version") { param($c, $t) $c -eq 0 -and $t -match "git version" }
        Programa "git cria o repositório na pasta" @($git, "init", "-q", $repo) { param($c, $t) $c -eq 0 -and (Test-Path (Join-Path $repo ".git")) }
        if (Test-Path $repo) {
            Set-Content -Path (Join-Path $repo "novo.txt") -Value "x"
            Programa "git vê o arquivo novo" @($git, "-C", $repo, "status", "--short") { param($c, $t) $t -eq "?? novo.txt" }
        }
        $repoFora = Join-Path $fora "repo-fora"
        Programa "git não cria repositório fora" @($git, "init", "-q", $repoFora) { param($c, $t) -not (Test-Path $repoFora) }
    }
    if ($node) {
        $js = Join-Path $permitida "grava.js"
        Set-Content -Path $js -Value 'require("fs").writeFileSync(process.argv[2], "node")'
        $n1 = Join-Path $permitida "do-node.txt"
        $n2 = Join-Path $fora "do-node.txt"
        Programa "node roda" @($node, "--version") { param($c, $t) $c -eq 0 -and $t -match "^v\d+" }
        Programa "node grava na pasta" @($node, $js, $n1) { param($c, $t) $c -eq 0 -and (Test-Path $n1) }
        Programa "node não grava fora" @($node, $js, $n2) { param($c, $t) -not (Test-Path $n2) }
        $leitura = Join-Path $permitida "le.js"
        Set-Content -Path $leitura -Value 'process.stdout.write(require("fs").readFileSync(process.argv[2], "utf8"))'
        Programa "node lê na pasta" @($node, $leitura, (Join-Path $permitida "a.txt")) { param($c, $t) $t -eq "dentro" }
        Programa "node não lê o que está fora" @($node, $leitura, (Join-Path $fora "b.txt")) { param($c, $t) $t -cne "fora" -and $c -ne 0 }
        # O localhost, que o AppContainer tiraria: o servidor de desenvolvimento
        # que a IA sobe e o retorno do login.
        $laco = Join-Path $permitida "laco.js"
        Set-Content -Path $laco -Value @'
const net = require("net");
const s = net.createServer((c) => c.end("pong")).listen(0, "127.0.0.1", () => {
  net.connect(s.address().port, "127.0.0.1").on("data", (d) => { process.stdout.write(String(d)); s.close(); });
});
'@
        Programa "localhost funciona" @($node, $laco) { param($c, $t) $t -eq "pong" }
    }
    # O PowerShell preso: é a ferramenta de shell do Claude Code quando o Git
    # Bash não sobe. Cada um só entra se está instalado.
    foreach ($nome in @("pwsh", "powershell")) {
        $exe = (Get-Command "$nome.exe" -ErrorAction SilentlyContinue).Source
        if (-not $exe) { continue }
        $base = @($exe, "-NoProfile", "-NonInteractive", "-Command")
        $p1 = Join-Path $permitida "do-$nome.txt"
        $p2 = Join-Path $fora "do-$nome.txt"
        Programa "$nome preso roda um comando" ($base + "Write-Output ola") { param($c, $t) $c -eq 0 -and $t -eq "ola" }
        Programa "$nome preso grava na pasta" ($base + "Set-Content -Path '$p1' -Value x") { param($c, $t) $c -eq 0 -and (Test-Path $p1) }
        Programa "$nome preso não grava fora" ($base + "Set-Content -Path '$p2' -Value x") { param($c, $t) -not (Test-Path $p2) }
        Programa "$nome preso lê na pasta" ($base + "Get-Content '$(Join-Path $permitida 'a.txt')'") { param($c, $t) $t -ceq "dentro" }
        Programa "$nome preso não lê o que está fora" ($base + "Get-Content '$(Join-Path $fora 'b.txt')'") { param($c, $t) $t -cne "fora" }
        Programa "$nome preso não lista a casa" ($base + "Get-ChildItem '$env:USERPROFILE' | ForEach-Object Name") { param($c, $t) $t -notmatch "Documents|Documentos|Desktop|AppData" }
        Programa "$nome preso sabe a pasta" ($base + 'Write-Output $env:KEEP_CONFINED_TO') { param($c, $t) $t -ieq $permitida }
        if ($git) { Programa "$nome preso chama o git" ($base + "git --version") { param($c, $t) $c -eq 0 -and $t -match "git version" } }
        if ($node) { Programa "$nome preso chama o node" ($base + "node --version") { param($c, $t) $c -eq 0 -and $t -match "^v\d+" } }
        Programa "$nome preso sobe um filho (cmd)" ($base + "cmd /d /c echo filho") { param($c, $t) $t -eq "filho" }
    }

    # O Claude Code de verdade, se está instalado (npm i -g @anthropic-ai/claude-code):
    # sobe preso, e sem login responde que falta o login, não que falta permissão.
    $claude = (Get-Command claude.cmd -ErrorAction SilentlyContinue).Source
    if (-not $claude) { $claude = (Get-Command claude.exe -ErrorAction SilentlyContinue).Source }
    if ($claude) {
        Programa "claude --version" @($claude, "--version") { param($c, $t) $c -eq 0 -and $t -match "\d+\.\d+\.\d+" } -mostrar
        Programa "claude -p chega ao pedido de login" @($claude, "-p", "ola") { param($c, $t) $t -match "(?i)log.?in|api key|authenticat|credential" -and $t -notmatch "(?i)EPERM|EACCES|access is denied|permission" } -mostrar
        # Com uma chave qualquer e um servidor que não existe, o Claude responde a
        # primeira linha (a lista de ferramentas) antes de a chamada falhar: preso,
        # ele tem a ferramenta PowerShell e não a Bash.
        $env:KEEP_WORKSPACE = "presa"
        $env:ANTHROPIC_API_KEY = "sk-ant-keep-teste-0000000000000000000000"
        $env:ANTHROPIC_BASE_URL = "http://127.0.0.1:9"
        $saida = Join-Path $trabalho "claude-init.jsonl"
        $erro = Join-Path $trabalho "claude-init.err"
        $filho = Start-Process -FilePath $Keep -ArgumentList @("ia", "confinar", "--", ('"' + $claude + '"'), "-p", "ola", "--output-format", "stream-json", "--verbose") -PassThru -WindowStyle Hidden -RedirectStandardOutput $saida -RedirectStandardError $erro
        $ferramentas = $null
        for ($i = 0; $i -lt 90 -and -not $ferramentas; $i++) {
            Start-Sleep -Milliseconds 500
            if (Test-Path $saida) {
                foreach ($linha in (Get-Content $saida -ErrorAction SilentlyContinue)) {
                    if ($linha -match '"subtype":"init"') { try { $ferramentas = @((ConvertFrom-Json $linha).tools) } catch { } ; break }
                }
            }
            if ($filho.HasExited) { break }
        }
        if (-not $filho.HasExited) { Stop-Process -Id $filho.Id -Force -ErrorAction SilentlyContinue }
        Remove-Item Env:\ANTHROPIC_API_KEY, Env:\ANTHROPIC_BASE_URL -ErrorAction SilentlyContinue
        if ($ferramentas) {
            Write-Host "        as ferramentas do Claude preso: $($ferramentas -join ', ')"
            Confere "o Claude preso tem a ferramenta PowerShell" "True" ($ferramentas -contains "PowerShell")
            Confere "o Claude preso não tem a ferramenta Bash" "False" ($ferramentas -contains "Bash")
        } else {
            Write-Host "  FALHOU  o Claude preso não listou as ferramentas"
            Write-Host "        a saída:"
            foreach ($l in @(Get-Content $saida -ErrorAction SilentlyContinue) + @(Get-Content $erro -ErrorAction SilentlyContinue)) { Write-Host "          $l" }
            $script:falhou++
        }
    }

    # O Gerenciador de Credenciais (onde o Claude Code pode guardar o login),
    # só para ver: o resultado vai para o registro, não conta.
    $cred = Roda "presa" @("cmd.exe", "/d", "/c", "cmdkey /generic:keep-confine-teste /user:u /pass:p >nul && cmdkey /list:keep-confine-teste")
    Write-Host ("  --    o Gerenciador de Credenciais preso: " + ($cred -replace '\s+', ' '))
    cmdkey /delete:keep-confine-teste 2>&1 | Out-Null

    if ($bash) {
        # O Git Bash (MSYS2) não sobe preso, e isto é um limite conhecido: ele cria
        # de início um canal de sinais com nome e uma DACL só do usuário e do
        # sistema, e a segunda checagem do token restrito (que não leva o usuário)
        # o recusa ("couldn't create signal pipe", erro 5). Só dito aqui, sem
        # contar: se um dia subir, o aviso do Keep ao subir o Claude pode sair.
        $env:KEEP_WORKSPACE = "presa"
        $saida = & $Keep ia confinar -- $bash -c "echo ola" 2>&1
        $codigo = $LASTEXITCODE
        $texto = (($saida | ForEach-Object { "$_" }) -join " ") -replace '\s+', ' '
        if ($codigo -eq 0 -and $texto -match "ola") {
            Write-Host "  --    o Git Bash passou a subir preso: o aviso do Keep ao subir o Claude pode sair"
        } else {
            Write-Host "  --    o Git Bash não sobe preso (limite conhecido): $texto"
        }
    }

    Write-Host "um workspace sem pasta"
    Confere "lê onde quiser" "fora" (Roda "livre" @("cmd.exe", "/d", "/c", "type", (Join-Path $fora "b.txt")))

    Write-Host "o que mandaria trabalho para fora do sandbox"
    $env:KEEP_WORKSPACE = "presa"
    & $Keep ia confinar -- claude agents 2>&1 | Out-Null
    Confere "claude agents é recusado" 1 $LASTEXITCODE
    & $Keep ia confinar -- codex queue x 2>&1 | Out-Null
    Confere "codex queue é recusado" 1 $LASTEXITCODE

    Write-Host "a pasta sumiu ou é larga demais"
    Grava-Config (Join-Path $trabalho "nao-existe")
    $env:KEEP_WORKSPACE = "presa"
    $saida = & $Keep ia confinar -- cmd.exe /d /c echo subiu 2>&1
    Confere "o programa não sobe" "False" ("$saida" -match "^subiu$")
    Confere "e diz por quê" "True" ("$saida" -match "não existe")
    Grava-Config $env:USERPROFILE
    $saida = & $Keep ia confinar -- cmd.exe /d /c echo subiu 2>&1
    Confere "a casa inteira não serve de pasta" "False" ("$saida" -match "^subiu$")
    Confere "e diz por quê" "True" ("$saida" -match "pasta do seu usuário")
}
finally {
    Remove-Item Env:\KEEP_CONFINE_CONFIG -ErrorAction SilentlyContinue
    Remove-Item Env:\KEEP_WORKSPACE -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $trabalho -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "passaram $passou, falharam $falhou"
exit ([int]($falhou -gt 0))
