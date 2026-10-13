function global:__KeepProgram([string]$name) {
  Get-Command -Name $name -CommandType Application,ExternalScript -All -ErrorAction SilentlyContinue | Select-Object -First 1
}
function global:__KeepClaudeStore($rest) {
  if (-not $env:KEEP_WORKSPACE -or $env:CLAUDE_SECURESTORAGE_CONFIG_DIR) { return $null }
  $first = ''
  if ($rest -and $rest.Count -gt 0) { $first = [string]$rest[0] }
  $quiet = '-v','--version','-h','--help','agents','attach','rc','remote-control','auth','auto-mode','config','daemon','doctor','gateway','help','import','install','kill','logs','mcp','plugin','plugins','project','respawn','rm','setup-token','stop','ultrareview','update','upgrade'
  if ($quiet -contains $first) { return $null }
  $keep = $env:KEEP_IA_BIN
  if (-not $keep -or -not (Test-Path -LiteralPath $keep)) { return $null }
  $answer = $null
  $old = [Console]::OutputEncoding
  try {
    [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
    $extra = @()
    if ($env:KEEP_IA_ESCOLHA) { $extra = @("--escolha=$($env:KEEP_IA_ESCOLHA)") }
    $line = & $keep ia preparar "--ws=$($env:KEEP_WORKSPACE)" "--shell=$PID" @extra --json 2>$null | Select-Object -Last 1
    if ($line) { $answer = $line | ConvertFrom-Json }
  } catch {
    $answer = $null
  } finally {
    try { [Console]::OutputEncoding = $old } catch {}
  }
  if ($answer -and $answer.ok -eq $true -and $answer.pasta) { return [string]$answer.pasta }
  $why = 'sem resposta do keep'
  if ($answer -and $answer.detalhe) { $why = [string]$answer.detalhe }
  Write-Host "Keep: não deu para preparar o login desta aba ($why); o Claude sobe como antes." -ForegroundColor Yellow
  return $null
}
function global:__KeepRun([string]$name, $rest) {
  $real = __KeepProgram $name
  if (-not $real) {
    Write-Error "The term '$name' is not recognized as the name of a cmdlet, function, script file, or executable program." -Category ObjectNotFound
    return
  }
  $store = $null
  if ($name -eq 'claude') { $store = __KeepClaudeStore $rest }
  if ($store) { $env:CLAUDE_SECURESTORAGE_CONFIG_DIR = $store }
  try {
    if ($MyInvocation.ExpectingInput) { $input | & $real @rest } else { & $real @rest }
  } finally {
    if ($store) { Remove-Item Env:CLAUDE_SECURESTORAGE_CONFIG_DIR -ErrorAction SilentlyContinue }
  }
}
if (-not (Test-Path -LiteralPath 'Function:\claude') -and -not (Test-Path -LiteralPath 'Alias:\claude')) {
  function global:claude {
    if ($MyInvocation.ExpectingInput) { $input | __KeepRun 'claude' $args } else { __KeepRun 'claude' $args }
  }
}
if (-not (Test-Path -LiteralPath 'Function:\codex') -and -not (Test-Path -LiteralPath 'Alias:\codex')) {
  function global:codex {
    if ($MyInvocation.ExpectingInput) { $input | __KeepRun 'codex' $args } else { __KeepRun 'codex' $args }
  }
}
