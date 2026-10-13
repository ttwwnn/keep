# Claude Code's status line command, for Keep on Windows: writes what Claude
# Code tells it (model, effort, session) to a file named after the tab's
# shell, so the Keep window can show them under the conversation. Prints
# nothing: Keep shows them, not the status line.
#
# The tab's shell is the ancestor whose parent is keepd. Outside a Keep tab
# there is none, and the script does nothing.
#
#   %USERPROFILE%\.local\state\keep\claude\<shell pid>.json
#   (or $env:KEEP_STATUS_DIR\<shell pid>.json)
#
# The app reads the same folder (src-tauri/src/status.rs). The installer puts
# this script beside the app, and the app sets it as Claude Code's status line
# when there is none (see keep-statusline.md). KEEP_STATUSLINE_TRACE, a file,
# gets a line for each run: the processes walked up, and what was written.

$trace = $env:KEEP_STATUSLINE_TRACE
function Trace([string]$line) {
    if ($trace) {
        try { Add-Content -LiteralPath $trace -Value ((Get-Date).ToString('o') + " $PID $line") } catch { }
    }
}

try {
    $dir = $env:KEEP_STATUS_DIR
    if (-not $dir) { $dir = Join-Path $env:USERPROFILE '.local\state\keep\claude' }

    # Claude Code hands over its JSON on stdin, in UTF-8.
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    $reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), $utf8)
    $json = $reader.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($json)) { Trace 'nothing on stdin'; exit 0 }

    # Who is whose child, in one question.
    $processes = @{}
    Get-CimInstance -ClassName Win32_Process -Property ProcessId, ParentProcessId, Name |
        ForEach-Object { $processes[[int]$_.ProcessId] = $_ }

    # Up from this script until the next one up is keepd: the one below it is the shell.
    $shell = 0
    $current = [int]$PID
    $walked = @()
    for ($hops = 0; $hops -lt 16; $hops++) {
        $self = $processes[$current]
        if (-not $self) { break }
        $walked += "$current $($self.Name)"
        $parent = [int]$self.ParentProcessId
        $above = $processes[$parent]
        if (-not $above) { break }
        if ($above.Name -like 'keepd*') { $shell = $current; $walked += "$parent $($above.Name)"; break }
        $current = $parent
    }
    if ($shell -le 0) { Trace ("no keepd above: " + ($walked -join ' < ')); exit 0 }

    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    # No byte order mark (PowerShell 5's Set-Content would write one), and a
    # file that appears whole: written aside, then moved over the old one.
    $temp = Join-Path $dir ('.in.' + $PID)
    $file = Join-Path $dir ("$shell.json")
    [System.IO.File]::WriteAllText($temp, $json, $utf8)
    Move-Item -Force -LiteralPath $temp -Destination $file
    Trace ("wrote $file; " + ($walked -join ' < '))
} catch {
    # A status line that fails must not disturb the conversation: Claude Code
    # shows what it prints, and an error goes to stderr, which it does not.
    Trace "failed: $_"
    [Console]::Error.WriteLine("keep-statusline: $_")
}
exit 0
