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
# The app reads the same folder (src-tauri/src/status.rs). To turn it on, see
# keep-statusline.md.

try {
    $dir = $env:KEEP_STATUS_DIR
    if (-not $dir) { $dir = Join-Path $env:USERPROFILE '.local\state\keep\claude' }

    # Claude Code hands over its JSON on stdin, in UTF-8.
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    $reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), $utf8)
    $json = $reader.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($json)) { exit 0 }

    # Who is whose child, in one question.
    $processes = @{}
    Get-CimInstance -ClassName Win32_Process -Property ProcessId, ParentProcessId, Name |
        ForEach-Object { $processes[[int]$_.ProcessId] = $_ }

    # Up from this script until the next one up is keepd: the one below it is the shell.
    $shell = 0
    $current = [int]$PID
    for ($hops = 0; $hops -lt 16; $hops++) {
        $self = $processes[$current]
        if (-not $self) { break }
        $parent = [int]$self.ParentProcessId
        $above = $processes[$parent]
        if (-not $above) { break }
        if ($above.Name -like 'keepd*') { $shell = $current; break }
        $current = $parent
    }
    if ($shell -le 0) { exit 0 }

    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    # No byte order mark (PowerShell 5's Set-Content would write one), and a
    # file that appears whole: written aside, then moved over the old one.
    $temp = Join-Path $dir ('.in.' + $PID)
    [System.IO.File]::WriteAllText($temp, $json, $utf8)
    Move-Item -Force -LiteralPath $temp -Destination (Join-Path $dir ("$shell.json"))
} catch {
    # A status line that fails must not disturb the conversation.
}
exit 0
