# The Windows app, end to end, on a real Windows: install it the way a person
# would, open it on a daemon of its own, let it run its scripted check, and
# photograph the screen. Used by CI; runs the same on any Windows machine.
#
#   pwsh apps/windows/e2e/run.ps1 [-Installer <setup.exe>] [-Out <dir>]
#
# Exit 0 only when the app reported every step done.
param(
    [string]$Installer = "",
    [string]$Out = "",
    [int]$TimeoutSeconds = 150
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

Write-Host '==> abrindo o Keep'
$keep = Start-Process -FilePath $app -PassThru
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while (-not (Test-Path $env:KEEP_E2E_REPORT) -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 500
}
Start-Sleep -Seconds 2

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
function Save-Screen([string]$path) {
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}
Save-Screen (Join-Path $Out 'tela.png')

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
Write-Host '==> ok'
