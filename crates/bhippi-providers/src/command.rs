//! Cross-platform command discovery and a deliberately small environment for provider CLIs.
//!
//! Windows package managers install launchers such as `claude.ps1` and `codex.ps1`, not
//! `*.exe`. Keeping launcher handling here prevents detection, installation, and prompt
//! execution from disagreeing about whether a provider exists.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The only environment a vendor CLI inherits (INV-003). Every entry is a deliberate
/// decision — widening this to the whole environment would leak credentials into child
/// processes, which is exactly what the scrub exists to prevent.
///
/// `PATHEXT` is load-bearing on Windows and must not be "tidied" away: npm installs its
/// launchers as PowerShell shims that call `node`, and without `PATHEXT` PowerShell cannot
/// resolve `node` to `node.exe`. The shim then fails silently and the child exits 0 with
/// empty stdout, which surfaces as "the CLI answered with nothing".
const SAFE_ENV_KEYS: &[&str] = &[
    "APPDATA",
    "COLORTERM",
    "COMSPEC",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "LANG",
    "LC_ALL",
    "LOCALAPPDATA",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PSMODULEPATH",
    "SHELL",
    "SYSTEMDRIVE",
    "SYSTEMROOT",
    "TEMP",
    "TERM",
    "TMP",
    "TMPDIR",
    "USERNAME",
    "USERPROFILE",
    "WINDIR",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

/// A directly executable binary or a PowerShell script with its interpreter fixed.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedCommand {
    program: PathBuf,
    prefix_args: Vec<OsString>,
    target: PathBuf,
}

impl ResolvedCommand {
    /// Builds a Tokio command with a scrubbed, functional environment (INV-003).
    ///
    /// Windows: `CREATE_NO_WINDOW` — background CLI spawns (chat answers, `--version`
    /// probes, installers) must never flash a console window in a desktop app.
    pub(crate) fn command(&self) -> tokio::process::Command {
        self.command_in(None)
    }

    /// Builds the scrubbed command inside an explicit project directory.
    pub(crate) fn command_in(&self, workspace: Option<&Path>) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command.args(&self.prefix_args);
        command.env_clear();
        for key in SAFE_ENV_KEYS {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.env("PATH", effective_path(self.target.parent()));
        command.env("NO_COLOR", "1");
        if let Some(workspace) = workspace {
            command.current_dir(workspace);
        } else if let Some(default_workspace) = agent_workspace() {
            command.current_dir(default_workspace);
        }
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.stdin(std::process::Stdio::null());
        // Every provider process is owned by the future or stream that launched it.
        // Tokio drops an `output()` future when a timeout wins; without this flag the
        // OS child survives that drop and account/model probes accumulate forever.
        // Chat streams set this again at their call site for emphasis, but putting the
        // guarantee here also covers detection, account probes, update checks, and npm.
        command.kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        command
    }

    #[must_use]
    pub(crate) fn target_exists(&self) -> bool {
        self.target.is_file()
    }
}

/// The one directory every provider CLI is launched in.
///
/// Vendor CLIs are agents: they read the directory they start in, and inheriting the
/// app's own working directory means a chat answer is shaped by whatever folder Bhippi
/// happened to be launched from — its `AGENTS.md`, its source, its git state. That is a
/// different product. One empty directory under the Bhippi data dir keeps chat answers
/// about the question, and gives Codex a stable place to run.
///
/// Resolved once: the directory outlives the process, so re-checking it per spawn buys
/// nothing. `None` (no home, or the directory cannot be made) falls back to inheriting,
/// which is what happened before this existed.
fn agent_workspace() -> Option<&'static Path> {
    static WORKSPACE: OnceLock<Option<PathBuf>> = OnceLock::new();
    WORKSPACE
        .get_or_init(|| {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            let dir = PathBuf::from(home).join(".bhippi").join("workspace");
            std::fs::create_dir_all(&dir).ok()?;
            Some(dir)
        })
        .as_deref()
}

/// Finds a provider or installer command in PATH plus stable per-user install locations.
/// The latter matters after an in-app install because a running desktop process does not
/// receive the terminal's newly refreshed PATH.
#[must_use]
pub(crate) fn resolve_command(name: &str) -> Option<ResolvedCommand> {
    let candidate = PathBuf::from(name);
    if candidate.components().count() > 1 && candidate.is_file() {
        return resolved_from_path(candidate);
    }
    // Native vendor binaries must win over npm's shell shims when we know their stable
    // package location. Grok's `.cmd` launcher truncates multi-line prompts; Windows
    // PowerShell 5's Codex launcher can re-tokenize one long prompt into separate words
    // when it forwards `$args`, which makes `codex exec` reject the second word as an
    // unexpected positional argument. Direct execution preserves Rust's argv boundaries.
    if matches!(name, "grok" | "codex") {
        if let Some(native) = resolve_native_vendor_exe(name) {
            return Some(native);
        }
    }
    resolve_in_dirs(name, &search_dirs())
}

/// Finds a launcher that transparently carries a long-lived stdin stream.
///
/// PowerShell's npm shim is required for multi-line argv chat prompts, but it does not
/// forward raw stdin to the Node child. Codex app-server speaks JSON Lines on stdin, so
/// that protocol must use a native executable — npm's `.cmd` wrapper buffers stdio and
/// the handshake never receives `account/read` / `account/rateLimits/read` replies.
#[must_use]
pub(crate) fn resolve_stdio_command(name: &str) -> Option<ResolvedCommand> {
    if let Some(native) = resolve_native_vendor_exe(name) {
        return Some(native);
    }
    #[cfg(windows)]
    {
        let names = [
            OsString::from(format!("{name}.exe")),
            OsString::from(format!("{name}.com")),
            OsString::from(format!("{name}.cmd")),
            OsString::from(format!("{name}.bat")),
            OsString::from(format!("{name}.ps1")),
        ];
        names
            .iter()
            .flat_map(|candidate| {
                search_dirs()
                    .into_iter()
                    .map(move |dir| dir.join(candidate))
            })
            .find(|candidate| candidate.is_file())
            .and_then(resolved_from_path)
    }
    #[cfg(not(windows))]
    resolve_command(name)
}

/// npm puts the real Codex binary several folders below the shim. Account probes must
/// talk to that binary; the shim's `cmd.exe` → `node` → `codex.js` chain swallows JSON-RPC.
fn resolve_native_vendor_exe(name: &str) -> Option<ResolvedCommand> {
    native_vendor_exe_candidates(name)
        .into_iter()
        .find(|path| path.is_file())
        .and_then(resolved_from_path)
}

fn native_vendor_exe_candidates(name: &str) -> Vec<PathBuf> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let mut paths = Vec::new();
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return paths;
    };
    let npm = PathBuf::from(appdata).join("npm").join("node_modules");
    if name == "grok" {
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            paths.push(PathBuf::from(home).join(".grok").join("bin").join(&file));
        }
    }
    if name == "codex" {
        for relative in [
            [
                "@openai",
                "codex",
                "node_modules",
                "@openai",
                "codex-win32-x64",
                "vendor",
                "x86_64-pc-windows-msvc",
                "bin",
            ]
            .as_slice(),
            [
                "@openai",
                "codex-win32-x64",
                "vendor",
                "x86_64-pc-windows-msvc",
                "bin",
            ]
            .as_slice(),
            [
                "@openai",
                "codex",
                "vendor",
                "x86_64-pc-windows-msvc",
                "bin",
            ]
            .as_slice(),
        ] {
            let mut path = npm.clone();
            for part in relative {
                path.push(part);
            }
            path.push(&file);
            paths.push(path);
        }
    }
    paths
}

fn resolve_in_dirs(name: &str, dirs: &[PathBuf]) -> Option<ResolvedCommand> {
    candidate_names(name)
        .iter()
        .flat_map(|candidate| dirs.iter().map(move |dir| dir.join(candidate)))
        .find(|candidate| candidate.is_file())
        .and_then(resolved_from_path)
}

fn resolved_from_path(target: PathBuf) -> Option<ResolvedCommand> {
    if cfg!(windows) && has_extension(&target, "cmd")
        || cfg!(windows) && has_extension(&target, "bat")
    {
        let shell = windows_cmd()?;
        return Some(ResolvedCommand {
            program: shell,
            prefix_args: vec![OsString::from("/c"), target.as_os_str().to_owned()],
            target,
        });
    }
    if cfg!(windows) && has_extension(&target, "ps1") {
        let powershell = windows_powershell()?;
        return Some(ResolvedCommand {
            program: powershell,
            prefix_args: vec![
                OsString::from("-NoLogo"),
                OsString::from("-NoProfile"),
                OsString::from("-NonInteractive"),
                OsString::from("-ExecutionPolicy"),
                OsString::from("Bypass"),
                OsString::from("-File"),
                target.as_os_str().to_owned(),
            ],
            target,
        });
    }
    Some(ResolvedCommand {
        program: target.clone(),
        prefix_args: Vec::new(),
        target,
    })
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case(wanted))
}

/// Launcher extensions in the order they are preferred on Windows.
///
/// A native executable first, then `.ps1` **ahead of** `.cmd`. That order is not a style
/// choice: npm ships both shims for the same tool, and the `.cmd` one forwards arguments
/// with `%*` through `cmd.exe`, where a raw newline ends the command line. A chat prompt
/// is always multi-line — system prompt, blank line, message — so the `.cmd` launcher
/// delivers its first line and silently drops the rest of the prompt *and* every flag
/// after it. The `.ps1` shim forwards `$args` as an array and carries the whole thing.
#[cfg(windows)]
const LAUNCHER_SUFFIXES: &[&str] = &[".exe", ".com", ".ps1", ".cmd", ".bat"];

fn candidate_names(name: &str) -> Vec<OsString> {
    let input = Path::new(name);
    if input.extension().is_some() {
        return vec![input.as_os_str().to_owned()];
    }
    let base_names: &[&str] = match name {
        "bionic" => &[
            "bionic",
            "Bionic",
            "bionic-cli",
            "bionic-gpt",
            "bionicai",
            "bionic_cli",
        ],
        "lmstudio" => &["lmstudio", "lms", "lm-studio", "LM-Studio"],
        _ => &[name],
    };
    let mut names = Vec::new();
    for base in base_names {
        #[cfg(windows)]
        {
            if *base == "npm" {
                names.push(OsString::from("npm.cmd"));
                names.push(OsString::from("npm.exe"));
                names.push(OsString::from("npm.ps1"));
            } else {
                for suffix in LAUNCHER_SUFFIXES {
                    names.push(OsString::from(format!("{base}{suffix}")));
                }
            }
        }
        #[cfg(not(windows))]
        {
            names.push(OsString::from(*base));
        }
    }
    names
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();

    if cfg!(windows) {
        push_env_child(&mut dirs, "APPDATA", &["npm"]);
        push_env_child(&mut dirs, "PROGRAMFILES", &["nodejs"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".local", "bin"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".cargo", "bin"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".claude", "local"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".codex", "bin"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".grok", "bin"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".bionic", "bin"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".bionic"]);
        push_env_child(&mut dirs, "USERPROFILE", &[".lmstudio", "bin"]);
        push_env_child(&mut dirs, "LOCALAPPDATA", &["Programs", "Bionic"]);
        push_env_child(&mut dirs, "LOCALAPPDATA", &["Programs", "Bionic", "bin"]);
        push_env_child(&mut dirs, "LOCALAPPDATA", &["Bionic"]);
        push_env_child(&mut dirs, "LOCALAPPDATA", &["LM-Studio", "bin"]);
        push_env_child(&mut dirs, "PROGRAMFILES", &["Bionic"]);
        push_env_child(&mut dirs, "PROGRAMFILES", &["BionicGPT"]);
    } else {
        push_env_child(&mut dirs, "HOME", &[".local", "bin"]);
        push_env_child(&mut dirs, "HOME", &[".cargo", "bin"]);
        push_env_child(&mut dirs, "HOME", &[".bionic", "bin"]);
        push_env_child(&mut dirs, "HOME", &[".bionic"]);
    }

    deduplicate_paths(dirs)
}

fn push_env_child(dirs: &mut Vec<PathBuf>, key: &str, children: &[&str]) {
    let Some(root) = std::env::var_os(key) else {
        return;
    };
    let mut path = PathBuf::from(root);
    for child in children {
        path.push(child);
    }
    dirs.push(path);
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| {
            let value = path.as_os_str().to_string_lossy();
            let key = if cfg!(windows) {
                value.to_ascii_lowercase()
            } else {
                value.into_owned()
            };
            seen.insert(key)
        })
        .collect()
}

fn effective_path(extra: Option<&Path>) -> OsString {
    let mut dirs = search_dirs();
    if let Some(path) = extra {
        dirs.insert(0, path.to_path_buf());
    }
    let dirs = deduplicate_paths(dirs);
    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

fn windows_cmd() -> Option<PathBuf> {
    let root = std::env::var_os("SYSTEMROOT")?;
    let path = PathBuf::from(root).join("System32").join("cmd.exe");
    path.is_file().then_some(path)
}

fn windows_powershell() -> Option<PathBuf> {
    let root = std::env::var_os("SYSTEMROOT")?;
    let path = PathBuf::from(root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::resolve_in_dirs;
    use super::{candidate_names, native_vendor_exe_candidates, ResolvedCommand};
    use std::ffi::OsString;
    #[cfg(windows)]
    use std::path::PathBuf;

    /// A private scratch directory, unique per process and per call.
    #[cfg(windows)]
    fn temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn codex_stdio_looks_inside_the_npm_vendor_tree() {
        let paths = native_vendor_exe_candidates("codex");
        if std::env::var_os("APPDATA").is_none() {
            assert!(paths.is_empty());
            return;
        }
        assert!(
            paths.iter().any(|path| path
                .to_string_lossy()
                .replace('\\', "/")
                .contains("codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex")),
            "{paths:?}"
        );
    }

    #[test]
    fn grok_native_exe_is_looked_up_under_the_user_grok_home() {
        let paths = native_vendor_exe_candidates("grok");
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
        if home.is_none() {
            assert!(paths.is_empty());
            return;
        }
        assert!(
            paths.iter().any(|path| path
                .to_string_lossy()
                .replace('\\', "/")
                .ends_with(".grok/bin/grok.exe")
                || path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .ends_with(".grok/bin/grok")),
            "{paths:?}"
        );
    }

    #[test]
    fn candidate_names_match_platform_launchers() {
        let names = candidate_names("codex");
        if cfg!(windows) {
            assert!(names.contains(&"codex.exe".into()));
            assert!(names.contains(&"codex.cmd".into()));
            assert!(names.contains(&"codex.ps1".into()));
            // npm ships both shims. `.cmd` forwards through cmd.exe, where a newline in
            // an argument ends the command line, so it must lose to `.ps1` — see
            // `a_multi_line_prompt_survives_the_launcher` for what that costs.
            let cmd_at = names.iter().position(|name| name == "codex.cmd");
            let ps1_at = names.iter().position(|name| name == "codex.ps1");
            assert!(ps1_at < cmd_at, "the .ps1 launcher must be preferred");
        } else {
            assert_eq!(names, vec![std::ffi::OsString::from("codex")]);
        }
    }

    #[test]
    fn an_explicit_project_workspace_wins_over_the_shared_default() {
        let executable = std::env::current_exe()
            .unwrap_or_else(|error| panic!("test executable path must resolve: {error}"));
        let resolved = ResolvedCommand {
            program: executable.clone(),
            prefix_args: Vec::<OsString>::new(),
            target: executable,
        };
        let workspace = std::env::temp_dir().join("bhippi-project-boundary-test");
        let command = resolved.command_in(Some(&workspace));

        assert_eq!(
            command.as_std().get_current_dir(),
            Some(workspace.as_path())
        );
    }

    /// Regression pin for the failure that made every CLI provider look broken.
    ///
    /// Every chat prompt is multi-line: system prompt, blank line, then the message. Sent
    /// through npm's `.cmd` shim, cmd.exe ends the command line at the first newline, so
    /// the vendor received one line of the prompt and none of the flags that followed it
    /// — and answered something plausible, which is why it read as the model being bad
    /// rather than as a launcher bug. Resolution must pick a launcher that carries the
    /// whole argument.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_multi_line_prompt_survives_the_launcher() {
        let root = temp_dir("bhippi-multiline-test");
        assert!(std::fs::create_dir_all(&root).is_ok());
        // Both shims exist side by side, exactly as npm installs them.
        assert!(std::fs::write(root.join("probe.cmd"), "@ECHO off\r\necho %*\r\n").is_ok());
        assert!(std::fs::write(
            root.join("probe.ps1"),
            "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
             Write-Output ($Rest -join '~')\n",
        )
        .is_ok());

        let Some(resolved) = resolve_in_dirs("probe", std::slice::from_ref(&root)) else {
            panic!("the probe shim must resolve");
        };
        let prompt = "line-one\n\nline-two";
        let Ok(output) = resolved
            .command()
            .arg(prompt)
            .arg("--trailing-flag")
            .output()
            .await
        else {
            panic!("the probe shim must run");
        };
        let seen = String::from_utf8_lossy(&output.stdout);
        assert!(seen.contains("line-one"), "saw {seen:?}");
        assert!(
            seen.contains("line-two"),
            "the prompt was truncated at its first newline; saw {seen:?}"
        );
        assert!(
            seen.contains("--trailing-flag"),
            "flags after the prompt were dropped; saw {seen:?}"
        );

        assert!(std::fs::remove_dir_all(root).is_ok());
    }

    /// Regression pin for the "the CLI answered with nothing" failure.
    ///
    /// npm's Windows launchers are shims that call `node`; without `PATHEXT` the shell
    /// cannot resolve `node` to `node.exe`, so the shim fails silently and the child exits
    /// 0 with empty stdout. Dropping `PATHEXT` from the scrub must fail here, loudly,
    /// rather than out in the product as an unexplained empty answer.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_spawned_cli_inherits_the_variables_a_node_shim_needs() {
        let root = std::env::temp_dir().join(format!(
            "bhippi-env-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        assert!(std::fs::create_dir_all(&root).is_ok());
        let shim = root.join("probe.ps1");
        assert!(std::fs::write(
            &shim,
            "Write-Output $env:PATHEXT
"
        )
        .is_ok());

        let resolved = resolve_in_dirs("probe", &[PathBuf::from(&root)]);
        let Some(resolved) = resolved else {
            panic!("the probe shim must resolve");
        };
        let output = resolved.command().output().await;
        let Ok(output) = output else {
            panic!("the probe shim must run");
        };
        let seen = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        assert!(
            seen.to_ascii_uppercase().contains(".EXE"),
            "PATHEXT must reach the child; saw {seen:?}"
        );

        assert!(std::fs::remove_file(shim).is_ok());
        assert!(std::fs::remove_dir(root).is_ok());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn resolves_and_launches_a_windows_npm_powershell_shim() {
        let root = std::env::temp_dir().join(format!(
            "bhippi-command-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        assert!(std::fs::create_dir_all(&root).is_ok());
        let shim = root.join("codex.ps1");
        assert!(std::fs::write(
            &shim,
            "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
                 Write-Output ($Rest -join '|')\n",
        )
        .is_ok());

        let resolved = resolve_in_dirs("codex", &[PathBuf::from(&root)]);
        assert!(resolved.is_some());
        let Some(resolved) = resolved else {
            return;
        };
        assert!(resolved.target_exists());
        let output = resolved.command().arg("--version").output().await;
        assert!(output.is_ok());
        if let Ok(output) = output {
            assert!(output.status.success());
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "--version");
        }

        assert!(std::fs::remove_file(shim).is_ok());
        assert!(std::fs::remove_dir(root).is_ok());
    }
}
