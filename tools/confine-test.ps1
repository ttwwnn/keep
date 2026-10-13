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

# Sem `$ErrorActionPreference = "Stop"`: no Windows PowerShell 5.1 a primeira linha
# que o `keep` escreve no erro (`Keep: …`), redirecionada por `2>&1`, seria uma
# exceção e acabaria o roteiro.
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
    # só entra se está instalado.
    Write-Host "programas de verdade"
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
        $env:KEEP_WORKSPACE = "presa"
        & $Keep ia confinar -- git init -q $repo 2>&1 | Out-Null
        Confere "git cria o repositório na pasta" "True" (Test-Path (Join-Path $repo ".git"))
        Set-Content -Path (Join-Path $repo "novo.txt") -Value "x"
        Confere "git vê o arquivo novo" "?? novo.txt" (Roda "presa" @("git", "-C", $repo, "status", "--short"))
        $env:KEEP_WORKSPACE = "presa"
        & $Keep ia confinar -- git init -q (Join-Path $fora "repo-fora") 2>&1 | Out-Null
        Confere "git não cria repositório fora" "False" (Test-Path (Join-Path $fora "repo-fora"))
    }
    if ($node) {
        $js = Join-Path $permitida "grava.js"
        Set-Content -Path $js -Value 'require("fs").writeFileSync(process.argv[2], "node")'
        $n1 = Join-Path $permitida "do-node.txt"
        $n2 = Join-Path $fora "do-node.txt"
        Roda "presa" @($node, $js, $n1) | Out-Null
        Confere "node grava na pasta" "True" (Test-Path $n1)
        Roda "presa" @($node, $js, $n2) | Out-Null
        Confere "node não grava fora" "False" (Test-Path $n2)
        $leitura = Join-Path $permitida "le.js"
        Set-Content -Path $leitura -Value 'process.stdout.write(require("fs").readFileSync(process.argv[2], "utf8"))'
        Confere "node lê na pasta" "dentro" (Roda "presa" @($node, $leitura, (Join-Path $permitida "a.txt")))
        Confere "node não lê o que está fora" "False" ((Roda "presa" @($node, $leitura, (Join-Path $fora "b.txt"))) -match "fora")
        # O localhost, que o AppContainer tiraria: o servidor de desenvolvimento
        # que a IA sobe e o retorno do login.
        $laco = Join-Path $permitida "laco.js"
        Set-Content -Path $laco -Value @'
const net = require("net");
const s = net.createServer((c) => c.end("pong")).listen(0, "127.0.0.1", () => {
  net.connect(s.address().port, "127.0.0.1").on("data", (d) => { process.stdout.write(String(d)); s.close(); });
});
'@
        Confere "localhost funciona" "pong" (Roda "presa" @($node, $laco))
    }
    if ($bash) {
        # O Git Bash (MSYS2) é o que o Claude Code usa para o Bash. Com outro
        # bash do usuário já aberto, que é o caso de quem usa o Git Bash.
        # As aspas são nossas: o Start-Process junta os argumentos sem pô-las.
        $aberto = Start-Process -FilePath $bash -ArgumentList @("-c", '"sleep 60"') -PassThru -WindowStyle Hidden
        Start-Sleep -Seconds 1
        Confere "o bash roda" "ola" (Roda "presa" @($bash, "-c", "echo ola"))
        $b1 = Join-Path $permitida "do-bash.txt"
        $b2 = Join-Path $fora "do-bash.txt"
        Roda "presa" @($bash, "-c", ("echo bash > '" + (Posix $b1) + "'")) | Out-Null
        Confere "o bash grava na pasta" "True" (Test-Path $b1)
        Roda "presa" @($bash, "-c", ("echo bash > '" + (Posix $b2) + "'")) | Out-Null
        Confere "o bash não grava fora" "False" (Test-Path $b2)
        Confere "o bash não lê o que está fora" "False" ((Roda "presa" @($bash, "-c", ("cat '" + (Posix (Join-Path $fora "b.txt")) + "'"))) -match "fora")
        Confere "o bash faz um filho e espera" "filho" (Roda "presa" @($bash, "-c", "(echo filho) | cat"))
        Stop-Process -Id $aberto.Id -Force -ErrorAction SilentlyContinue
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
