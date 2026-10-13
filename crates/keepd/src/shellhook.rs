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
//! `codex` that goes through the same door, where the folder-bound launch of
//! the macOS app will come in (`keep ia confinar`).
//!
//! What it does, in a tab of the Keep with no login store set:
//!
//!  - `claude` asks `keep ia preparar --ws=<workspace> --shell=<pid>
//!    [--escolha=<KEEP_IA_ESCOLHA>] --json` and, with a folder in the answer,
//!    runs the real Claude with `CLAUDE_SECURESTORAGE_CONFIG_DIR` set to it
//!    for that run alone;
//!  - when the core refuses or does not answer it says so in one line and
//!    runs Claude as it would have: a failure here never keeps Claude from
//!    starting;
//!  - what opens no conversation (`claude mcp …`, `claude update`,
//!    `--version`…) is passed through untouched, and so is a Claude opened
//!    from inside another (the variable is inherited);
//!  - piped input, arguments (the empty ones too) and the exit code reach the
//!    real program as they would have.
//!
//! The real program is the one PowerShell would have run without the
//! function: the first `claude` in the path that is a program or a script,
//! `claude.exe` of the native install or the npm `claude.cmd`. Defined only
//! when the person's profile has no function or alias of that name. Cmd and
//! Git Bash have no such hook (a tab on them runs Claude on the global
//! login), and WSL is a Linux the daemon cannot see into.
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
            let dir = std::env::temp_dir().join(format!("keepd-pwsh-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            std::fs::create_dir_all(dir.join("home")).unwrap();
            std::fs::write(dir.join("ai.ps1"), AI).unwrap();
            // What the real programs say about the way they were run.
            let tell = "echo \"[$(basename $0)] n=$#\"\nfor a in \"$@\"; do echo \"  arg=[$a]\"; done\n\
                        echo \"  store=[$CLAUDE_SECURESTORAGE_CONFIG_DIR]\"\n\
                        if [ -n \"$KEEP_TEST_STDIN\" ]; then echo \"  stdin=[$(cat)]\"; fi\nexit ${KEEP_TEST_EXIT:-0}\n";
            write_script(&dir.join("bin/claude"), tell);
            write_script(&dir.join("bin/codex"), tell);
            // The core: logs what it was asked, answers as told.
            write_script(
                &dir.join("keep"),
                "echo \"keep $*\" >> \"$KEEP_TEST_LOG\"\n\
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

        // No such program: PowerShell's own complaint, not a crash.
        let (out, _) = w.run(&tab, "", "$env:PATH = '/usr/bin:/bin'; claude hi");
        assert!(out.contains("The term 'claude' is not recognized"), "{out}");
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
