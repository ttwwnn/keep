# The Windows app, end to end, on a real Windows: install it the way a person
# would, open it on a daemon of its own, let it run its scripted check, and
# photograph the screen. Used by CI; runs the same on any Windows machine.
#
#   pwsh apps/windows/e2e/run.ps1 [-Installer <setup.exe>] [-Out <dir>] [-NucleoReal]
#
# The AI layer is checked too (docs/ia.md): the usage footer on a made-up home
# against made-up services, and — through a stand-in core for what the core
# does not answer yet (e2e/ia-falso.mjs), or the real one with -NucleoReal —
# a tab's AI menu, a switch that asks first, a login, and a close that sends a
# worktree to the Recycle Bin. Each moment worth seeing is photographed when
# the app asks (telas/<name>.png).
#
# Exit 0 only when the app reported every step done.
param(
    [string]$Installer = "",
    [string]$Out = "",
    [int]$TimeoutSeconds = 300,
    [switch]$NucleoReal
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
if (-not $Out) { $Out = Join-Path $here 'out' }
New-Item -ItemType Directory -Force $Out | Out-Null

if (-not $Installer) {
    $Installer = Get-ChildItem (Join-Path $here '..\src-tauri\target\release\bundle\nsis') -Filter '*.exe' |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Installer) { throw 'instalador não encontrado' }
Write-Host "==> instalando $Installer"
$p = Start-Process -FilePath $Installer -ArgumentList '/S' -PassThru -Wait
if ($p.ExitCode -ne 0) { throw "o instalador saiu com $($p.ExitCode)" }

$candidates = @(
    (Join-Path $env:LOCALAPPDATA 'Keep\Keep.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\Keep\Keep.exe')
)
$app = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $app) {
    $app = Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'Keep.exe' -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $app) { throw 'Keep.exe não foi instalado' }
$dir = Split-Path $app
Write-Host "==> instalado em $dir"
Get-ChildItem -Recurse $dir | Select-Object FullName, Length | Format-Table -AutoSize | Out-String | Write-Host

# A daemon, a state and a report of this run's own: nothing a person has open
# is touched, and nothing left from another run is read.
$env:KEEP_SOCKET = "keep-e2e-$PID"
$env:KEEP_STATE_DIR = Join-Path $Out 'state'
$env:KEEP_E2E_REPORT = Join-Path $Out 'report.json'
Remove-Item -Force -ErrorAction SilentlyContinue $env:KEEP_E2E_REPORT

# The AI layer on a made-up home: two Claude logins (the global one and a
# login folder of the Keep's) and a GPT one, whose owners and usage a
# made-up service on 127.0.0.1 answers. The core reads only this home and
# asks only that service (KEEP_IA_*), so no account of anybody's is touched.
$falso = Join-Path $here 'ia-falso.mjs'
$env:KEEP_IA_HOME = Join-Path $Out 'casa-ia'
$env:KEEP_IA_ESTADO = Join-Path $Out 'estado-ia'
& node $falso casa $env:KEEP_IA_HOME
if ($LASTEXITCODE -ne 0) { throw 'não consegui montar a casa falsa' }
$portFile = Join-Path $Out 'porta-ia.txt'
Remove-Item -Force -ErrorAction SilentlyContinue $portFile
$servico = Start-Process -FilePath node -ArgumentList @("`"$falso`"", 'servidor', "`"$portFile`"") -PassThru -WindowStyle Hidden
$until = (Get-Date).AddSeconds(15)
while (-not (Test-Path $portFile) -and (Get-Date) -lt $until) { Start-Sleep -Milliseconds 200 }
if (-not (Test-Path $portFile)) { throw 'o serviço falso não abriu' }
$porta = (Get-Content $portFile -Raw).Trim()
$env:KEEP_IA_PERFIL_URL = "http://127.0.0.1:$porta/profile"
$env:KEEP_IA_CLAUDE_URL = "http://127.0.0.1:$porta/claude"
$env:KEEP_IA_CODEX_URL = "http://127.0.0.1:$porta/codex"
$env:KEEP_IA_GERENTE_EXTERNO = '0'
Write-Host "==> serviço falso de IA em 127.0.0.1:$porta"

# A worktree for the close question to send to the Recycle Bin — on the
# system drive, whose Bin is there for certain.
$base = Join-Path $env:TEMP "keep-e2e-ia-$PID"
$repo = Join-Path $base 'repo-ia'
$worktree = Join-Path $base 'wt-ia'
& git init -q $repo
& git -C $repo -c user.name=e2e -c user.email=e2e@keep.local commit -q --allow-empty -m inicio
& git -C $repo worktree add -q -b e2e-wt $worktree
Set-Content -Path (Join-Path $worktree 'nota.txt') -Value 'e2e'
if (-not (Test-Path (Join-Path $worktree '.git'))) { throw 'não consegui criar a worktree de teste' }

if ($NucleoReal) {
    $env:KEEP_E2E_IA = 'real'
} else {
    # What the core does not answer yet comes from the stand-in; what it
    # does (accounts, usage, order) still goes to the installed keep.exe.
    $env:KEEP_E2E_IA = 'falso'
    $env:KEEP_IA_BIN = Join-Path $here 'keep-falso.cmd'
    $env:KEEP_E2E_REAL_KEEP = Join-Path $dir 'bin\keep.exe'
    $env:KEEP_E2E_FAKE_STATE = Join-Path $Out 'ia-falso.json'
    @{ caminhoWorktree = $worktree } | ConvertTo-Json | Set-Content -Path $env:KEEP_E2E_FAKE_STATE
}

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
function Save-Screen([string]$path) {
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}

# The photographs the app asks for, mid-run: <name>.pedido in, <name>.png out.
$telas = Join-Path $Out 'telas'
function Serve-Shots {
    if (-not (Test-Path $telas)) { return }
    Get-ChildItem $telas -Filter '*.pedido' -ErrorAction SilentlyContinue | ForEach-Object {
        Start-Sleep -Milliseconds 250
        Save-Screen (Join-Path $telas ($_.BaseName + '.png'))
        Remove-Item -Force -ErrorAction SilentlyContinue $_.FullName
        Write-Host "    tela $($_.BaseName)"
    }
}

Write-Host '==> abrindo o Keep'
$keep = Start-Process -FilePath $app -PassThru
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while (-not (Test-Path $env:KEEP_E2E_REPORT) -and (Get-Date) -lt $deadline) {
    Serve-Shots
    Start-Sleep -Milliseconds 200
}
Start-Sleep -Seconds 2
Save-Screen (Join-Path $Out 'tela.png')

# The worktree the close sent away: gone from its place, and in the Recycle
# Bin — the stand-in wrote down where the app said it went.
$lixeira = 'sem conferência'
if (-not $NucleoReal) {
    $estado = Get-Content $env:KEEP_E2E_FAKE_STATE -Raw | ConvertFrom-Json
    $destino = $estado.concluido.destino
    $saiu = -not (Test-Path $worktree)
    $naLixeira = $destino -and ($destino -match '\$Recycle\.Bin\\[^\\]+\\\$R') -and (Test-Path -LiteralPath $destino)
    $lixeira = "saiu do lugar: $saiu; destino: $destino; está na Lixeira: $naLixeira"
    if (-not ($saiu -and $naLixeira)) { $lixeira = "FALHOU: $lixeira" }
    Write-Host "==> worktree: $lixeira"
    Copy-Item -ErrorAction SilentlyContinue "$($env:KEEP_E2E_FAKE_STATE).chamadas.log" (Join-Path $Out 'ia-chamadas.txt')
}
Set-Content -Path (Join-Path $Out 'lixeira.txt') -Value $lixeira
if ($servico) { Stop-Process -Id $servico.Id -Force -ErrorAction SilentlyContinue }

$cli = Join-Path $dir 'bin\keep.exe'
if (Test-Path $cli) {
    & $cli ls 2>&1 | Tee-Object -FilePath (Join-Path $Out 'keep-ls.txt') | Write-Host
}
Get-Process | Where-Object { $_.ProcessName -in @('Keep', 'keepd', 'OpenConsole', 'conhost', 'pwsh', 'powershell') } |
    Select-Object ProcessName, Id, Path | Format-Table -AutoSize | Out-String |
    Tee-Object -FilePath (Join-Path $Out 'processos.txt') | Write-Host

# An update with the daemon running: the app closes, the installer runs over
# it, and the daemon — with the tabs in it — must still be there afterwards,
# running from the copy the installer moved aside.
Stop-Process -Id $keep.Id -Force -ErrorAction SilentlyContinue
$daemon = Get-Process keepd -ErrorAction SilentlyContinue | Select-Object -First 1
$update = 'sem daemon para conferir'
if ($daemon) {
    Write-Host "==> atualizando por cima, com o keepd $($daemon.Id) rodando"
    $p = Start-Process -FilePath $Installer -ArgumentList '/S' -PassThru -Wait
    $alive = Get-Process -Id $daemon.Id -ErrorAction SilentlyContinue
    $aside = Get-ChildItem (Join-Path $dir 'bin') -Filter 'keepd.antigo-*.exe' -ErrorAction SilentlyContinue
    $tabs = if (Test-Path $cli) { (& $cli ls 2>&1) -join "`n" } else { '' }
    $update = "instalador saiu com $($p.ExitCode); keepd vivo: $([bool]$alive); copia antiga: $([bool]$aside); novo keepd.exe: $(Test-Path (Join-Path $dir 'bin\keepd.exe'))"
    Write-Host $update
    Write-Host $tabs
    $kept = $tabs -and ($tabs -notmatch 'no workspaces')
    $update += "; abas mantidas: $kept"
    if ($p.ExitCode -ne 0 -or -not $alive -or -not $aside -or -not $kept -or -not (Test-Path (Join-Path $dir 'bin\keepd.exe'))) {
        $update = "FALHOU: $update"
    }
}
Set-Content -Path (Join-Path $Out 'atualizacao.txt') -Value $update

# Leaving: this run's daemon, by the pipe name it was given — every keepd
# here is this run's.
Get-Process keepd -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

if (-not (Test-Path $env:KEEP_E2E_REPORT)) {
    $log = Join-Path $env:LOCALAPPDATA 'Keep\keepd.log'
    if (Test-Path $log) { Write-Host "keepd.log: $(Get-Content $log -Raw)" }
    throw "o app não entregou o relatório em $TimeoutSeconds s"
}
$report = Get-Content $env:KEEP_E2E_REPORT -Raw | ConvertFrom-Json
$report.steps | Format-Table name, ok, ms, detail -AutoSize | Out-String -Width 220 | Write-Host
if (-not $report.ok) { throw 'o roteiro do app falhou' }
if ($update -like 'FALHOU*') { throw "a atualização com o daemon rodando falhou: $update" }
if ($lixeira -like 'FALHOU*') { throw "a worktree não foi para a Lixeira: $lixeira" }
Write-Host '==> ok'
