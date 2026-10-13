function global:__KeepProgram([string]$name) {
  Get-Command -Name $name -CommandType Application,ExternalScript -All -ErrorAction SilentlyContinue | Select-Object -First 1
}
function global:__KeepKind([string]$name, $rest) {
  if ($name -ne 'claude') { return 'conversation' }
  $first = ''
  if ($rest -and $rest.Count -gt 0) { $first = [string]$rest[0] }
  $held = 'agents','attach','rc','remote-control'
  $plain = '-v','--version','-h','--help','auth','auto-mode','config','daemon','doctor','gateway','help','import','install','kill','logs','mcp','plugin','plugins','project','respawn','rm','setup-token','stop','ultrareview','update','upgrade'
  if ($held -contains $first) { return 'held' }
  if ($plain -contains $first) { return 'plain' }
  return 'conversation'
}
function global:__KeepClaudeStore {
  if (-not $env:KEEP_WORKSPACE -or $env:CLAUDE_SECURESTORAGE_CONFIG_DIR) { return $null }
  $keep = $env:KEEP_IA_BIN
  if (-not $keep -or -not (Test-Path -LiteralPath $keep -PathType Leaf)) { return $null }
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
function global:__KeepHold([string]$name) {
  $keep = $env:KEEP_IA_BIN
  if (-not $env:KEEP_WORKSPACE -or -not $keep -or -not (Test-Path -LiteralPath $keep -PathType Leaf)) { return $null }
  $app = Get-Command -Name $name -CommandType Application -All -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($app) { return $app.Source }
  return $null
}
function global:__KeepRun([string]$name, $rest) {
  $real = __KeepProgram $name
  if (-not $real) {
    Write-Error "The term '$name' is not recognized as the name of a cmdlet, function, script file, or executable program." -Category ObjectNotFound
    return
  }
  $kind = __KeepKind $name $rest
  $store = $null
  if ($name -eq 'claude' -and $kind -eq 'conversation') { $store = __KeepClaudeStore }
  if ($store) { $env:CLAUDE_SECURESTORAGE_CONFIG_DIR = $store }
  $hold = $null
  if ($kind -ne 'plain') { $hold = __KeepHold $name }
  try {
    if ($hold) {
      $line = @('ia','confinar','--',$hold)
      if ($null -ne $rest) { $line += $rest }
      if ($MyInvocation.ExpectingInput) { $input | & $env:KEEP_IA_BIN @line } else { & $env:KEEP_IA_BIN @line }
    } elseif ($MyInvocation.ExpectingInput) { $input | & $real @rest } else { & $real @rest }
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
