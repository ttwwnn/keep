//! What the tests type, said in the shell they run.
//!
//! The tests were written for a POSIX shell, and on unix they still type what
//! they always did. On Windows each line is the same request made of
//! PowerShell, with the property the originals were careful to have: what a
//! test waits for is computed by the shell, so it can only appear once the
//! command has run — never in the shell's echo of the command being typed.

#![allow(dead_code)]

use portable_pty::CommandBuilder;

/// The shell a test tab runs when the test spawns it itself.
pub fn shell() -> CommandBuilder {
    #[cfg(unix)]
    {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("PS1", "$ ");
        cmd
    }
    #[cfg(windows)]
    {
        let mut cmd = CommandBuilder::new("powershell.exe");
        cmd.args(["-NoLogo", "-NoProfile"]);
        cmd
    }
}

/// Make the daemon's own tabs run the test shell too. On unix they run the
/// user's shell, as they always have.
pub fn use_test_shell() {
    #[cfg(windows)]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // Edition 2021: still a safe call, and every test in the binary
            // wants the same value.
            std::env::set_var("KEEP_SHELL", "powershell.exe -NoLogo -NoProfile");
        });
    }
}

/// A command line, with the key that runs it.
pub fn line(command: &str) -> Vec<u8> {
    #[cfg(unix)]
    let enter = "\n";
    // A console reads the return key as a carriage return.
    #[cfg(windows)]
    let enter = "\r";
    format!("{command}{enter}").into_bytes()
}

/// Print `tag` followed by `a + b`, and the text that proves it ran.
pub fn computed(tag: &str, a: u32, b: u32) -> (Vec<u8>, String) {
    #[cfg(unix)]
    let command = format!("echo {tag}$(({a}+{b}))");
    #[cfg(windows)]
    let command = format!("Write-Output ('{tag}' + ({a}+{b}))");
    (line(&command), format!("{tag}{}", a + b))
}

/// Print each of `lines` on a line of its own.
pub fn print_lines(lines: &[&str]) -> Vec<u8> {
    #[cfg(unix)]
    let command = format!("printf '{}\\n'", lines.join("\\n"));
    #[cfg(windows)]
    let command = format!(
        "Write-Output {}",
        lines.iter().map(|l| format!("'{l}'")).collect::<Vec<_>>().join(",")
    );
    line(&command)
}

/// `DANGER` in bold red.
pub fn styled() -> Vec<u8> {
    #[cfg(unix)]
    let command = r"printf '\033[1;31mDANGER\033[0m\n'";
    #[cfg(windows)]
    let command = "Write-Output ([char]27 + '[1;31mDANGER' + [char]27 + '[0m')";
    line(command)
}

/// Four ticks a little apart, then `fin22`.
pub fn ticks() -> Vec<u8> {
    #[cfg(unix)]
    let command = "for i in 1 2 3 4; do echo tick-$i; sleep 0.15; done; echo fin$((20+2))";
    #[cfg(windows)]
    let command = "foreach ($i in 1..4) { Write-Output \"tick-$i\"; Start-Sleep -Milliseconds 150 }; Write-Output ('fin' + (20+2))";
    line(command)
}

/// A command that runs, as a process of its own, for about `seconds`.
///
/// On Windows not `Start-Sleep`, which PowerShell runs inside itself: a
/// command that starts no process is invisible to the process tree, which is
/// all Windows has to say what a tab is running.
pub fn wait_a_while(seconds: u32) -> Vec<u8> {
    #[cfg(unix)]
    let command = format!("sleep {seconds}");
    #[cfg(windows)]
    let command = format!("ping -n {} 127.0.0.1 > $null", seconds + 1);
    line(&command)
}

/// The name a running `wait_a_while` reports.
pub const WAITING_COMMAND: &str = if cfg!(windows) { "ping" } else { "sleep" };

/// Far more output than a client's backlog holds.
pub fn flood() -> Vec<u8> {
    #[cfg(unix)]
    let command = "seq 1 200000";
    // PowerShell prints a line at a time, a hundred times slower than `seq`:
    // a fifth as many is still far past the backlog, and done in seconds.
    #[cfg(windows)]
    let command = "1..40000";
    line(command)
}

/// Three hundred numbered lines.
pub fn count_to_300() -> Vec<u8> {
    #[cfg(unix)]
    let command = "seq 1 300";
    #[cfg(windows)]
    let command = "1..300";
    line(command)
}

/// Set a title, then hold it for a few seconds.
pub fn titled(title: &str) -> Vec<u8> {
    #[cfg(unix)]
    let command = format!("printf '\\033]2;{title}\\007'; sleep 3");
    #[cfg(windows)]
    let command = format!(
        "Write-Host -NoNewline ([char]27 + ']2;{title}' + [char]7); Start-Sleep 3"
    );
    line(&command)
}

/// A directory to `cd` into, as the system reports it, and the line that
/// goes there.
pub fn elsewhere() -> (Vec<u8>, String) {
    #[cfg(unix)]
    {
        // `/tmp` is a symlink to `/private/tmp` on macOS, and what comes back
        // is the resolved path either way.
        let target = std::fs::canonicalize("/tmp").expect("resolve /tmp");
        (line("cd /tmp"), target.to_str().expect("utf-8 path").to_owned())
    }
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        (line(&format!("Set-Location '{root}'")), root)
    }
}

/// Whether a screen shows the shell waiting at its prompt.
pub fn shows_prompt(screen: &str) -> bool {
    #[cfg(unix)]
    return screen.contains('$') || screen.contains('%') || screen.contains('❯');
    #[cfg(windows)]
    return screen.contains("PS ") && screen.contains('>');
}

/// What a tab at its prompt reports as its command.
pub fn is_shell_name(name: &str) -> bool {
    #[cfg(unix)]
    return matches!(name, "sh" | "bash");
    #[cfg(windows)]
    return name == "powershell";
}

/// Replace the shell with a program that waits, and the name it reports.
pub fn become_waiting_program() -> (Vec<u8>, &'static str) {
    #[cfg(unix)]
    return (line("exec cat"), "cat");
    #[cfg(windows)]
    return (line("ping -n 30 127.0.0.1"), "ping");
}

/// Print `awake` after a second, typed once, with nothing typed after.
pub fn later(word: &str) -> Vec<u8> {
    #[cfg(unix)]
    let command = format!("(sleep 1; echo {word}) &");
    #[cfg(windows)]
    let command = format!("Start-Sleep 1; Write-Output '{word}'");
    line(&command)
}

/// A program that prints a word and exits, spawned without a shell.
pub fn one_shot(word: &str) -> CommandBuilder {
    #[cfg(unix)]
    {
        let mut cmd = CommandBuilder::new("/bin/echo");
        cmd.arg(word);
        cmd
    }
    #[cfg(windows)]
    {
        let mut cmd = CommandBuilder::new("cmd.exe");
        cmd.args(["/c", "echo", word]);
        cmd
    }
}
