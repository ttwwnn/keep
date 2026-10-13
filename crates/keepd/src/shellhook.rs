//! What the shell of a tab on Windows learns, besides where it is (the prompt
//! hook in `workspace.rs`): the Mac's `claude` and `codex` of the zsh, in
//! PowerShell, and where to find `keep`.
//!
//! On the Mac the author's shell has a `claude()` function that asks the core
//! for the tab's own login store (`keep ia preparar`, docs/ia.md "Troca a
//! quente") and sets `CLAUDE_SECURESTORAGE_CONFIG_DIR` to it for that one
//! Claude: the account of the tab is then the store's, and switching it is
//! rewriting the store, with the Claude open and the conversation going.
//! Without the function a `claude` typed in a tab runs on the global login
//! and the switch is refused (`claude-global`). `ai.ps1` is that function
//! for PowerShell — the shell a tab of the Keep runs on Windows — and a
//! `codex` that goes through the same door.
//!
//! What it does, in a tab of the Keep:
//!
//!  - `claude`, with no login store set, asks `keep ia preparar
//!    --ws=<workspace> --shell=<pid> [--escolha=<KEEP_IA_ESCOLHA>] --json`
//!    and, with a folder in the answer, runs the real Claude with
//!    `CLAUDE_SECURESTORAGE_CONFIG_DIR` set to it for that run alone;
//!  - when the core refuses or does not answer it says so in one line and
//!    runs Claude as it would have: a failure here never keeps Claude from
//!    starting;
//!  - what opens no conversation (`claude mcp …`, `claude update`,
//!    `--version`…) is passed through untouched, and so is a Claude opened
//!    from inside another (the variable is inherited, so it gets no store);
//!    `claude agents|attach|rc|remote-control` get no store either;
//!  - ready to hold the AI to a folder, and off: with `__KeepConfining` made
//!    to answer `$true` — one line of `ai.ps1`, `$false` as shipped, until
//!    the confinement is proven on Windows (`CONFINEMENT_ENFORCED` in the
//!    app) — both programs are run by `keep ia confinar -- <program> …`, as
//!    in the Mac's functions: the AI of a workspace with a folder chosen for
//!    it is held to that folder (docs/ia.md), and with none the core runs the
//!    program as it would have. The function then never falls back to
//!    running it unheld when the core refuses to hold it: what the core said
//!    is the answer. The program given to the core is a file it can start —
//!    the `claude.exe` or the npm `claude.cmd`, not a `claude.ps1`
//!    PowerShell would have preferred. `claude agents|attach|rc|remote-control`
//!    and a Claude opened from inside another go through it too (the core
//!    refuses the ones that send work to a server outside the folder);
//!  - piped input, arguments (the empty ones too) and the exit code reach the
//!    real program as they would have.
//!
//! Run directly — as shipped, always; once the hold is on, outside a tab of
//! the Keep, for what opens no conversation, with no `keep` to ask, or when
//! the only `claude` found is a script of PowerShell (which no one holds to a
//! folder) — the real program is the one PowerShell would have run without
//! the function: the first `claude` in the path that is a program or a
//! script, `claude.exe` of the native install or the npm `claude.cmd`.
//! Defined only when the person's profile has no function or alias of that
//! name. Cmd and Git Bash have no such hook (a tab on them runs Claude on the
//! global login, and unheld), and WSL is a Linux the daemon cannot see into.
//!
//! PowerShell 5.1 and 7 both run it. The core's answer is read as UTF-8
//! whatever the console's code page is: a folder under a name with an accent
//! (`C:\Users\José`) is not read back as the console's idea of its bytes.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;

/// The functions, run after the prompt hook.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const AI: &str = include_str!("ai.ps1");

/// `keep` is beside the daemon's own executable, in the folder the installer
/// makes. A tab learns it two ways: `KEEP_IA_BIN`, which the function above
/// runs (and which a test or a person may have set already: theirs stands),
/// and the end of its path, so that `keep nova` and `keep ls` are there to
/// type — not first, so that no other `keep` is shadowed.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn with_core(cmd: &mut CommandBuilder) {
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        with_core_in(cmd, &dir);
    }
}

fn with_core_in(cmd: &mut CommandBuilder, dir: &Path) {
    let Some(keep) = core_in(dir) else { return };
    if cmd.get_env("KEEP_IA_BIN").is_none() {
        cmd.env("KEEP_IA_BIN", &keep);
    }
    let path = cmd.get_env("PATH").map(OsStr::to_os_string);
    cmd.env("PATH", path_with(dir, path.as_deref()));
}

fn core_in(dir: &Path) -> Option<PathBuf> {
    let keep = dir.join(format!("keep{}", std::env::consts::EXE_SUFFIX));
    keep.is_file().then_some(keep)
}

/// `path` (the value of a PATH variable) with `dir` at its end, unless it is
/// one of its folders already — whatever the slashes or, on Windows, the case.
fn path_with(dir: &Path, path: Option<&OsStr>) -> OsString {
    let mut dirs: Vec<PathBuf> = path.map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
    if !dirs.iter().any(|d| same_dir(d, dir)) {
        dirs.push(dir.to_path_buf());
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| path.map(OsString::from).unwrap_or_default())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let plain = |p: &Path| {
        let text = p.to_string_lossy();
        let text = text.trim_end_matches(['/', '\\']);
        if cfg!(windows) { text.replace('/', "\\").to_lowercase() } else { text.to_string() }
    };
    plain(a) == plain(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keepd-shellhook-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_folder_of_keep_goes_to_the_end_of_the_path_once() {
        let dir = Path::new("/opt/keep/bin");
        let joined = |dirs: &[&str]| std::env::join_paths(dirs).unwrap();
        let split = |p: OsString| std::env::split_paths(&p).map(|d| d.display().to_string()).collect::<Vec<_>>();
        assert_eq!(split(path_with(dir, Some(&joined(&["/usr/bin", "/bin"])))), ["/usr/bin", "/bin", "/opt/keep/bin"]);
        // Already there, in front or behind, with or without the last slash: nothing is added.
        assert_eq!(split(path_with(dir, Some(&joined(&["/opt/keep/bin", "/bin"])))), ["/opt/keep/bin", "/bin"]);
        assert_eq!(split(path_with(dir, Some(&joined(&["/bin", "/opt/keep/bin/"])))), ["/bin", "/opt/keep/bin/"]);
        // No path at all: the folder alone.
        assert_eq!(split(path_with(dir, None)), ["/opt/keep/bin"]);
    }

    #[test]
    fn a_tab_learns_where_keep_is_without_taking_the_place_of_the_persons_choice() {
        let dir = scratch("core");
        let keep = dir.join(format!("keep{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&keep, b"").unwrap();
        let mut cmd = CommandBuilder::new("shell");
        cmd.env_clear();
        cmd.env("PATH", std::env::join_paths(["/usr/bin"]).unwrap());
        with_core_in(&mut cmd, &dir);
        assert_eq!(cmd.get_env("KEEP_IA_BIN"), Some(keep.as_os_str()));
        let path = cmd.get_env("PATH").unwrap().to_os_string();
        assert_eq!(std::env::split_paths(&path).last().as_deref(), Some(dir.as_path()));
        // A `KEEP_IA_BIN` already there — a test's, a person's — stays.
        let mut chosen = CommandBuilder::new("shell");
        chosen.env_clear();
        chosen.env("KEEP_IA_BIN", "/outro/keep");
        with_core_in(&mut chosen, &dir);
        assert_eq!(chosen.get_env("KEEP_IA_BIN"), Some(OsStr::new("/outro/keep")));
        // No `keep` beside the daemon (a build tree with only the daemon): nothing is set.
        let empty = scratch("core-empty");
        let mut bare = CommandBuilder::new("shell");
        bare.env_clear();
        with_core_in(&mut bare, &empty);
        assert_eq!(bare.get_env("KEEP_IA_BIN"), None);
        assert_eq!(bare.get_env("PATH"), None);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn the_script_stays_small_enough_for_a_command_line() {
        // Handed to PowerShell as an encoded command (UTF-16, then base64) on a command line of 32,767 characters,
        // with the prompt hook and the shell's own arguments before it.
        let prompt = 1_000;
        let bytes = (prompt + AI.encode_utf16().count()) * 2;
        let encoded = bytes.div_ceil(3) * 4;
        assert!(encoded < 16_000, "{encoded}");
    }
}

/// The functions in a PowerShell of their own, against stand-ins of `keep`,
/// `claude` and `codex`: what they run, with what, and what is left behind.
/// Needs a `pwsh` (`KEEP_TEST_PWSH` names one, else the path's); without it
/// the test says so and passes.
#[cfg(all(test, unix))]
mod in_powershell {
    use super::AI;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    fn pwsh() -> Option<PathBuf> {
        if let Some(named) = std::env::var_os("KEEP_TEST_PWSH") {
            return Some(PathBuf::from(named));
        }
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join("pwsh")).find(|p| p.is_file())
    }

    /// The line of `ai.ps1` that decides whether `claude` and `codex` go through `keep ia confinar`, as shipped
    /// (off, while the confinement is off in the app: `CONFINEMENT_ENFORCED`), and as it is edited to turn it on.
    const HOLD_OFF: &str = "function global:__KeepConfining { $false }";
    const HOLD_ON: &str = "function global:__KeepConfining { $true }";

    /// The script with the hold turned on by that one edit.
    fn holding() -> String {
        assert_eq!(AI.matches(HOLD_OFF).count(), 1, "the switch is one line of the script");
        AI.replace(HOLD_OFF, HOLD_ON)
    }

    fn write_script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A world of stand-ins in `dir`: programs to find, a log of what the core was asked.
    struct World {
        dir: PathBuf,
        pwsh: PathBuf,
    }

    impl World {
        fn new(name: &str, pwsh: PathBuf) -> World {
            World::with_script(name, pwsh, AI)
        }

        fn with_script(name: &str, pwsh: PathBuf, script: &str) -> World {
            let dir = std::env::temp_dir().join(format!("keepd-pwsh-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            std::fs::create_dir_all(dir.join("home")).unwrap();
            std::fs::write(dir.join("ai.ps1"), script).unwrap();
            // What the real programs say about the way they were run.
            let tell = "echo \"[$(basename $0)] n=$#\"\nfor a in \"$@\"; do echo \"  arg=[$a]\"; done\n\
                        echo \"  store=[$CLAUDE_SECURESTORAGE_CONFIG_DIR]\"\n\
                        if [ -n \"$KEEP_TEST_STDIN\" ]; then echo \"  stdin=[$(cat)]\"; fi\nexit ${KEEP_TEST_EXIT:-0}\n";
            write_script(&dir.join("bin/claude"), tell);
            write_script(&dir.join("bin/codex"), tell);
            // The core: logs what it was asked, answers as told, and runs what it is told to hold (a core with
            // no folder for the workspace runs the program as it would have; `KEEP_TEST_HOLD=refuse` is one that
            // will not).
            write_script(
                &dir.join("keep"),
                "echo \"keep $*\" >> \"$KEEP_TEST_LOG\"\n\
                 if [ \"$1 $2\" = \"ia confinar\" ]; then\n\
                 if [ \"$KEEP_TEST_HOLD\" = refuse ]; then echo 'Keep: a pasta permitida nao existe.' >&2; exit 1; fi\n\
                 shift 2; if [ \"$1\" = \"--\" ]; then shift; fi\n\
                 exec \"$@\"\n\
                 fi\n\
                 case \"$KEEP_TEST_CORE\" in\n\
                 ok) printf '{\"versao\":1,\"ok\":true,\"pasta\":\"/tmp/José/abas/1-2\",\"conta\":\"claude:ana\"}\\n' ;;\n\
                 refuse) printf '{\"versao\":1,\"ok\":false,\"motivo\":\"sem-conta\",\"detalhe\":\"Nao ha conta.\"}\\n'; exit 1 ;;\n\
                 garbage) echo 'panic: algo'; exit 101 ;;\n\
                 esac\n",
            );
            World { dir, pwsh }
        }

        /// Run `commands` after the functions are loaded; what was printed, and what the core was asked.
        fn run(&self, env: &[(&str, &str)], before: &str, commands: &str) -> (String, String) {
            let log = self.dir.join("core.log");
            let _ = std::fs::remove_file(&log);
            let script = format!("{before}\n. '{}'\n{commands}", self.dir.join("ai.ps1").display());
            let mut cmd = Command::new(&self.pwsh);
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .env_clear()
                .env("PATH", format!("{}:/usr/bin:/bin", self.dir.join("bin").display()))
                .env("HOME", self.dir.join("home"))
                .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
                .env("KEEP_TEST_LOG", &log)
                .stdin(Stdio::null());
            for (k, v) in env {
                cmd.env(k, v);
            }
            let out = cmd.output().expect("pwsh");
            let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
            (text, std::fs::read_to_string(&log).unwrap_or_default())
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn claude_runs_on_the_login_store_of_the_tab_and_codex_and_the_rest_pass_through() {
        let Some(pwsh) = pwsh() else {
            eprintln!("sem pwsh (KEEP_TEST_PWSH ou no PATH): pulado");
            return;
        };
        let w = World::new("ai", pwsh);
        let keep = w.dir.join("keep");
        let tab = [("KEEP_WORKSPACE", "w1"), ("KEEP_IA_BIN", keep.to_str().unwrap()), ("KEEP_TEST_CORE", "ok")];

        // In a tab: the core is asked who the tab is, the answer's folder is the login store for that run only,
        // the arguments go as they are (the empty one and the one with a space too), and nothing is left behind.
        let (out, asked) = w.run(&tab, "", "claude --foo 'bar baz' ''; Write-Host \"after=[$env:CLAUDE_SECURESTORAGE_CONFIG_DIR]\"");
        assert!(out.contains("[claude] n=3") && out.contains("  arg=[--foo]") && out.contains("  arg=[bar baz]") && out.contains("  arg=[]"), "{out}");
        assert!(out.contains("  store=[/tmp/José/abas/1-2]"), "a folder under an accent is read as UTF-8: {out}");
        assert!(out.contains("after=[]"), "{out}");
        assert!(asked.starts_with("keep ia preparar --ws=w1 --shell=") && asked.trim_end().ends_with("--json"), "{asked}");
        assert!(!asked.contains("--escolha"), "{asked}");

        // The tab's choice goes along.
        let mut chosen = tab.to_vec();
        chosen.push(("KEEP_IA_ESCOLHA", "claude:trabalho"));
        let (_, asked) = w.run(&chosen, "", "claude -p oi");
        assert!(asked.contains("--escolha=claude:trabalho"), "{asked}");

        // What opens no conversation passes through untouched: the core is not asked, no store is set.
        for line in ["claude mcp list", "claude --version", "claude update", "claude auth login"] {
            let (out, asked) = w.run(&tab, "", line);
            assert!(out.contains("  store=[]") && asked.is_empty(), "{line}: {out} {asked}");
        }

        // Outside a tab of the Keep, or with the store set already (a Claude opened from inside another): untouched.
        let (out, asked) = w.run(&[("KEEP_IA_BIN", keep.to_str().unwrap()), ("KEEP_TEST_CORE", "ok")], "", "claude hi");
        assert!(out.contains("  store=[]") && asked.is_empty(), "{out} {asked}");
        let mut set = tab.to_vec();
        set.push(("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/tmp/ja"));
        let (out, asked) = w.run(&set, "", "claude hi");
        assert!(out.contains("  store=[/tmp/ja]") && asked.is_empty(), "{out} {asked}");

        // The core says no, or says nothing sensible: one line says so, and Claude runs as it would have.
        for (core, why) in [("refuse", "Nao ha conta."), ("garbage", "sem resposta do keep")] {
            let mut t = tab.to_vec();
            t[2] = ("KEEP_TEST_CORE", core);
            let (out, _) = w.run(&t, "", "claude hi");
            assert!(out.contains(&format!("Keep: não deu para preparar o login desta aba ({why}); o Claude sobe como antes.")), "{core}: {out}");
            assert!(out.contains("[claude] n=1") && out.contains("  store=[]"), "{core}: {out}");
        }

        // No core to ask (no `keep`, a wrong path): Claude as it would have, and no complaint.
        let (out, _) = w.run(&[("KEEP_WORKSPACE", "w1"), ("KEEP_IA_BIN", "/nao/existe/keep")], "", "claude hi");
        assert!(out.contains("[claude] n=1") && !out.contains("Keep:"), "{out}");

        // Piped input reaches the program, and its exit code reaches the shell.
        let mut piped = tab.to_vec();
        piped.push(("KEEP_TEST_STDIN", "1"));
        piped.push(("KEEP_TEST_EXIT", "7"));
        let (out, _) = w.run(&piped, "", "'entrada de teste' | claude -p; Write-Host \"code=$LASTEXITCODE\"");
        assert!(out.contains("  stdin=[entrada de teste]") && out.contains("code=7"), "{out}");

        // Codex goes through the same door, with nothing to prepare.
        let (out, asked) = w.run(&tab, "", "codex resume abc");
        assert!(out.contains("[codex] n=2") && out.contains("  arg=[resume]") && out.contains("  store=[]") && asked.is_empty(), "{out} {asked}");

        // As shipped, neither goes through `keep ia confinar`, whatever that would say: the confinement is off
        // until it is proven on Windows. A core that would refuse to hold changes nothing.
        let mut refusing = tab.to_vec();
        refusing.push(("KEEP_TEST_HOLD", "refuse"));
        for line in ["claude hi", "codex hi", "claude agents"] {
            let (out, asked) = w.run(&refusing, "", line);
            assert!(out.contains("] n=") && !out.contains("Keep: a pasta") && !asked.contains("confinar"), "{line}: {out} {asked}");
        }

        // No such program: PowerShell's own complaint, not a crash.
        let (out, _) = w.run(&tab, "", "$env:PATH = '/usr/bin:/bin'; claude hi");
        assert!(out.contains("The term 'claude' is not recognized"), "{out}");
    }

    /// The hold, switched on by the one-line edit: what the function does the day the confinement is proven.
    #[test]
    fn what_the_core_is_to_hold_is_never_run_unheld() {
        let Some(pwsh) = pwsh() else {
            eprintln!("sem pwsh (KEEP_TEST_PWSH ou no PATH): pulado");
            return;
        };
        let w = World::with_script("hold", pwsh, &holding());
        let keep = w.dir.join("keep");
        let tab = [("KEEP_WORKSPACE", "w1"), ("KEEP_IA_BIN", keep.to_str().unwrap()), ("KEEP_TEST_CORE", "ok")];

        // In a tab: the core is asked to prepare the login store and then to hold the program — a file it can
        // start, with the arguments after it (the empty one and the one with a space too) — and the store is the
        // one the first answer gave.
        let (out, asked) = w.run(&tab, "", "claude --foo 'bar baz' ''");
        let lines: Vec<&str> = asked.lines().collect();
        assert_eq!(lines.len(), 2, "{asked}");
        assert!(lines[0].starts_with("keep ia preparar --ws=w1 --shell="), "{asked}");
        let held = format!("keep ia confinar -- {}/claude --foo bar baz", w.dir.join("bin").display());
        assert!(lines[1].starts_with(&held), "{asked}");
        assert!(out.contains("[claude] n=3") && out.contains("  arg=[bar baz]") && out.contains("  store=[/tmp/José/abas/1-2]"), "{out}");

        // A Claude opened from inside another has its store already: no preparing, but held all the same. Outside a
        // tab of the Keep, or with no `keep` to ask, nothing is held; what opens no conversation never is.
        let mut set = tab.to_vec();
        set.push(("CLAUDE_SECURESTORAGE_CONFIG_DIR", "/tmp/ja"));
        let (out, asked) = w.run(&set, "", "claude hi");
        assert!(out.contains("  store=[/tmp/ja]") && !asked.contains("ia preparar") && asked.contains("ia confinar -- "), "{out} {asked}");
        let (out, asked) = w.run(&[("KEEP_IA_BIN", keep.to_str().unwrap()), ("KEEP_TEST_CORE", "ok")], "", "claude hi; codex hi");
        assert!(out.contains("[claude] n=1") && out.contains("[codex] n=1") && asked.is_empty(), "{out} {asked}");
        let (out, asked) = w.run(&[("KEEP_WORKSPACE", "w1"), ("KEEP_IA_BIN", "/nao/existe/keep")], "", "claude hi; codex hi");
        assert!(out.contains("[claude] n=1") && out.contains("[codex] n=1") && asked.is_empty(), "{out} {asked}");
        for line in ["claude mcp list", "claude --version", "claude update"] {
            let (_, asked) = w.run(&tab, "", line);
            assert!(asked.is_empty(), "{line}: {asked}");
        }

        // The core will not hold it (the folder is gone): nothing runs, the core's word reaches the person, and the
        // shell's exit code is the core's.
        let mut refusing = tab.to_vec();
        refusing.push(("KEEP_TEST_HOLD", "refuse"));
        for line in ["claude hi", "codex hi", "claude agents"] {
            let (out, _) = w.run(&refusing, "", &format!("{line}; Write-Host \"code=$LASTEXITCODE\""));
            assert!(out.contains("Keep: a pasta permitida nao existe.") && out.contains("code=1") && !out.contains("] n="), "{line}: {out}");
        }
        // What opens no conversation is not held, so the refusal never reaches it.
        let (out, _) = w.run(&refusing, "", "claude --version");
        assert!(out.contains("[claude] n=1") && !out.contains("Keep:"), "{out}");

        // `agents`, `attach` and `rc` get no store but go through the core, which refuses the ones that send work
        // to a server outside the folder.
        for line in ["claude agents", "claude attach abc", "claude rc", "claude remote-control"] {
            let (out, asked) = w.run(&tab, "", line);
            assert!(out.contains("  store=[]") && !asked.contains("ia preparar") && asked.contains("ia confinar -- "), "{line}: {out} {asked}");
        }

        // No arguments at all reach the program as none, and a `--` between quotes (PowerShell's own is eaten
        // before a function sees it) as the argument it is.
        let (out, asked) = w.run(&tab, "", "claude");
        assert!(out.contains("[claude] n=0"), "{out}");
        assert!(asked.lines().last().unwrap().trim_end().ends_with("/claude"), "{asked}");
        let (out, _) = w.run(&tab, "", "claude '--' 'a b'");
        assert!(out.contains("[claude] n=2") && out.contains("  arg=[--]") && out.contains("  arg=[a b]"), "{out}");

        // A script of PowerShell in front in the path is what PowerShell would run, and not a program the core can
        // start: the one beside it is what is held.
        let scripts = w.dir.join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(scripts.join("claude.ps1"), "'o script rodou'\n").unwrap();
        let before = format!("$env:PATH = '{}:' + $env:PATH", scripts.display());
        let (out, asked) = w.run(&tab, &before, "claude hi");
        assert!(out.contains("[claude] n=1") && !out.contains("o script rodou"), "{out}");
        assert!(asked.contains(&format!("{}/claude hi", w.dir.join("bin").display())), "{asked}");
        let (out, asked) = w.run(&tab, &before, "claude --version");
        assert!(out.contains("o script rodou") && !asked.contains("ia confinar"), "{out} {asked}");
    }

    /// The script as a tab hands it to PowerShell: UTF-16, in base64, as an encoded command.
    fn encoded(script: &str) -> String {
        const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(TABLE[(n >> 18) as usize & 63] as char);
            out.push(TABLE[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
            out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
        }
        out
    }

    #[test]
    fn handed_over_as_an_encoded_command_the_accents_of_its_message_survive() {
        let Some(pwsh) = pwsh() else {
            eprintln!("sem pwsh (KEEP_TEST_PWSH ou no PATH): pulado");
            return;
        };
        let w = World::new("encoded", pwsh);
        let keep = w.dir.join("keep");
        let out = Command::new(&w.pwsh)
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded(&format!("{AI}\nclaude hi"))])
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", w.dir.join("bin").display()))
            .env("HOME", w.dir.join("home"))
            .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
            .env("KEEP_TEST_LOG", w.dir.join("core.log"))
            .env("KEEP_TEST_CORE", "refuse")
            .env("KEEP_WORKSPACE", "w1")
            .env("KEEP_IA_BIN", &keep)
            .stdin(Stdio::null())
            .output()
            .expect("pwsh");
        let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("Keep: não deu para preparar o login desta aba (Nao ha conta.); o Claude sobe como antes."), "{text}");
        assert!(text.contains("[claude] n=1"), "{text}");
    }

    #[test]
    fn a_function_or_alias_of_the_persons_is_never_replaced() {
        let Some(pwsh) = pwsh() else {
            eprintln!("sem pwsh (KEEP_TEST_PWSH ou no PATH): pulado");
            return;
        };
        let w = World::new("own", pwsh);
        let keep = w.dir.join("keep");
        let tab = [("KEEP_WORKSPACE", "w1"), ("KEEP_IA_BIN", keep.to_str().unwrap()), ("KEEP_TEST_CORE", "ok")];
        let (out, asked) = w.run(&tab, "function claude { 'a minha' }\nSet-Alias codex Write-Output", "claude hi; codex 'o meu'");
        assert!(out.contains("a minha") && out.contains("o meu") && !out.contains("[claude]") && asked.is_empty(), "{out} {asked}");
    }
}
