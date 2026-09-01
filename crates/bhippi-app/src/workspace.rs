//! Project-first workspace commands for the desktop ADE.
//!
//! Filesystem and process behavior stays in Rust (R3). Project records are references:
//! removing one from Bhippi never deletes the directory it points at.

use crate::commands::AppError;
use bhippi_core::ProjectRecord;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProjectSummary {
    pub name: String,
    pub path: String,
    pub is_git_repository: bool,
    pub branch: Option<String>,
    pub active: bool,
    pub last_opened_at: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTool {
    VsCode,
    Cursor,
    Antigravity,
    Explorer,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ToolAvailability {
    pub tool: ProjectTool,
    pub label: String,
    pub available: bool,
    pub hint: String,
}

fn now_timestamp() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

pub(crate) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
        if let Some(stripped) = s.strip_prefix("//?/") {
            return PathBuf::from(stripped);
        }
    }
    path.to_path_buf()
}

fn canonical_directory(raw: &str) -> Result<PathBuf, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::plain("Choose a project folder first."));
    }
    let path = std::fs::canonicalize(trimmed).map_err(|error| AppError {
        message: format!("Project folder is unavailable: {error}"),
        hint: Some("Check that the folder exists and that Bhippi can read it.".to_owned()),
    })?;
    if !path.is_dir() {
        return Err(AppError::plain("The project path must be a folder."));
    }
    Ok(strip_verbatim_prefix(&path))
}

pub(crate) fn display_path(path: &Path) -> String {
    strip_verbatim_prefix(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Project")
        .to_owned()
}

async fn branch_for(path: &Path) -> Option<String> {
    if !path.join(".git").exists() {
        return None;
    }
    let output = tokio::process::Command::new("git")
        .args(["-C", &display_path(path), "branch", "--show-current"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn paths_match(a: &str, b: &str) -> bool {
    crate::chat::ChatEngine::paths_match(a, b)
}

async fn summaries(config: &bhippi_core::BhippiConfig) -> Vec<ProjectSummary> {
    let mut rows: Vec<ProjectSummary> = Vec::with_capacity(config.workspace.projects.len());
    for project in &config.workspace.projects {
        let normalized = display_path(&strip_verbatim_prefix(&PathBuf::from(&project.path)));
        let is_active = config
            .workspace
            .active_project
            .as_deref()
            .is_some_and(|act| paths_match(act, &normalized));

        if let Some(existing) = rows.iter_mut().find(|r| paths_match(&r.path, &normalized)) {
            if project.last_opened_at > existing.last_opened_at {
                existing.last_opened_at = project.last_opened_at;
            }
            if is_active {
                existing.active = true;
            }
            continue;
        }

        let path = PathBuf::from(&normalized);
        let is_git_repository = path.join(".git").exists();
        rows.push(ProjectSummary {
            name: project.name.clone(),
            path: normalized,
            is_git_repository,
            branch: branch_for(&path).await,
            active: is_active,
            last_opened_at: project.last_opened_at,
        });
    }
    rows.sort_by_key(|row| std::cmp::Reverse(row.last_opened_at));
    rows
}

async fn remember_project(
    state: &crate::Runtime,
    path: PathBuf,
) -> Result<ProjectSummary, AppError> {
    let stored_path = display_path(&path);
    let name = project_name(&path);
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config
        .workspace
        .projects
        .retain(|project| !paths_match(&project.path, &stored_path));
    for project in &mut config.workspace.projects {
        project.path = display_path(&strip_verbatim_prefix(&PathBuf::from(&project.path)));
    }
    config.workspace.projects.push(ProjectRecord {
        name: name.clone(),
        path: stored_path.clone(),
        last_opened_at: now_timestamp(),
    });
    config.workspace.active_project = Some(stored_path.clone());
    state.config.save(&config).await.map_err(AppError::from)?;
    Ok(ProjectSummary {
        name,
        path: stored_path,
        is_git_repository: path.join(".git").exists(),
        branch: branch_for(&path).await,
        active: true,
        last_opened_at: now_timestamp(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn list_projects(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<ProjectSummary>, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    Ok(summaries(&config).await)
}

#[tauri::command]
#[specta::specta]
pub async fn add_existing_project(
    state: tauri::State<'_, crate::Runtime>,
    path: String,
) -> Result<ProjectSummary, AppError> {
    let canonical = canonical_directory(&path)?;
    remember_project(state.inner(), canonical).await
}

fn validate_project_name(name: &str) -> Result<&str, AppError> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(AppError::plain("Enter a project name."));
    }
    if name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        return Err(AppError::plain(
            "Project names cannot contain path separators or control characters.",
        ));
    }
    Ok(name)
}

#[tauri::command]
#[specta::specta]
pub async fn create_project(
    state: tauri::State<'_, crate::Runtime>,
    parent: String,
    name: String,
) -> Result<ProjectSummary, AppError> {
    let parent = canonical_directory(&parent)?;
    let name = validate_project_name(&name)?;
    let target = parent.join(name);
    if target.exists() {
        return Err(AppError::plain(
            "A file or folder with that project name already exists.",
        ));
    }
    tokio::fs::create_dir(&target)
        .await
        .map_err(|error| AppError {
            message: format!("Could not create the project folder: {error}"),
            hint: Some("Choose a writable parent folder and try again.".to_owned()),
        })?;
    let canonical = canonical_directory(&display_path(&target))?;
    remember_project(state.inner(), canonical).await
}

fn validate_git_url(url: &str) -> Result<&str, AppError> {
    let url = url.trim();
    if url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git@")
    {
        Ok(url)
    } else {
        Err(AppError {
            message: "Enter a Git HTTPS or SSH URL.".to_owned(),
            hint: Some("Example: https://github.com/owner/repository.git".to_owned()),
        })
    }
}

#[tauri::command]
#[specta::specta]
pub async fn clone_project(
    state: tauri::State<'_, crate::Runtime>,
    git_url: String,
    parent: String,
) -> Result<ProjectSummary, AppError> {
    let url = validate_git_url(&git_url)?;
    let parent = canonical_directory(&parent)?;
    let output = tokio::process::Command::new("git")
        .arg("clone")
        .arg("--")
        .arg(url)
        .current_dir(&parent)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| AppError {
            message: format!("Git could not start: {error}"),
            hint: Some("Install Git, then retry the clone.".to_owned()),
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(AppError {
            message: "Git could not clone that repository.".to_owned(),
            hint: Some(if detail.is_empty() {
                "Check the URL, access rights, and network connection.".to_owned()
            } else {
                detail
            }),
        });
    }
    let repo_name = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("project")
        .trim_end_matches(".git");
    let cloned = canonical_directory(&display_path(&parent.join(repo_name)))?;
    remember_project(state.inner(), cloned).await
}

#[tauri::command]
#[specta::specta]
pub async fn select_project(
    state: tauri::State<'_, crate::Runtime>,
    path: String,
) -> Result<ProjectSummary, AppError> {
    let canonical = canonical_directory(&path)?;
    remember_project(state.inner(), canonical).await
}

#[tauri::command]
#[specta::specta]
pub async fn forget_project(
    state: tauri::State<'_, crate::Runtime>,
    path: String,
) -> Result<Vec<ProjectSummary>, AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    // Removing a project is the one point where dropping its unsaved engine sessions is
    // what the user asked for; switching projects deliberately keeps them.
    if let Ok(mut sessions) = crate::engine::sessions().lock() {
        sessions.close_project(std::path::Path::new(path.trim()));
    }
    config
        .workspace
        .projects
        .retain(|project| !paths_match(&project.path, path.trim()));
    if config
        .workspace
        .active_project
        .as_deref()
        .is_some_and(|active| paths_match(active, path.trim()))
    {
        config.workspace.active_project = config.workspace.projects.first().map(|p| p.path.clone());
    }
    state.config.save(&config).await.map_err(AppError::from)?;
    Ok(summaries(&config).await)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path).find_map(|directory| {
        extensions.iter().find_map(|ext| {
            let candidate = if name.ends_with(ext) {
                directory.join(name)
            } else {
                directory.join(format!("{name}{ext}"))
            };
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

#[cfg(windows)]
fn executable_on_path(name: &str) -> bool {
    find_on_path(name).is_some()
}

fn dirs_in_local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn find_cursor() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = dirs_in_local_appdata();
        let prog_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
        let prog_files_x86 = std::env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Program Files (x86)"));

        let candidates = [
            local.join("Programs\\cursor\\_\\Cursor.exe"),
            local.join("Programs\\cursor\\Cursor.exe"),
            local.join("Programs\\cursor\\resources\\app\\bin\\cursor.cmd"),
            local.join("Programs\\cursor\\bin\\cursor.cmd"),
            prog_files.join("Cursor\\_\\Cursor.exe"),
            prog_files.join("Cursor\\Cursor.exe"),
            prog_files.join("Cursor\\resources\\app\\bin\\cursor.cmd"),
            prog_files.join("Cursor\\bin\\cursor.cmd"),
            prog_files_x86.join("Cursor\\Cursor.exe"),
        ];
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(found);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            PathBuf::from("/Applications/Cursor.app/Contents/MacOS/Cursor"),
            PathBuf::from("/Applications/Cursor.app/Contents/Resources/app/bin/cursor"),
        ];
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(found);
        }
    }

    find_on_path("cursor")
}

fn find_antigravity() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = dirs_in_local_appdata();
        let prog_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Program Files"));

        let candidates = [
            local.join("Programs\\Antigravity IDE\\Antigravity IDE.exe"),
            local.join("Programs\\antigravity\\Antigravity.exe"),
            local.join("Programs\\antigravity\\bin\\antigravity.cmd"),
            prog_files.join("Antigravity IDE\\Antigravity IDE.exe"),
            prog_files.join("Antigravity\\Antigravity.exe"),
        ];
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(found);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            PathBuf::from("/Applications/Antigravity.app/Contents/MacOS/Antigravity"),
            PathBuf::from("/Applications/Antigravity IDE.app/Contents/MacOS/Antigravity IDE"),
        ];
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(found);
        }
    }

    find_on_path("antigravity")
}

fn find_vscode() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = dirs_in_local_appdata();
        let prog_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
        let prog_files_x86 = std::env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Program Files (x86)"));

        let candidates = [
            local.join("Programs\\Microsoft VS Code\\Code.exe"),
            local.join("Programs\\Microsoft VS Code\\bin\\code.cmd"),
            prog_files.join("Microsoft VS Code\\Code.exe"),
            prog_files.join("Microsoft VS Code\\bin\\code.cmd"),
            prog_files_x86.join("Microsoft VS Code\\Code.exe"),
            prog_files_x86.join("Microsoft VS Code\\bin\\code.cmd"),
        ];
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(found);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            PathBuf::from("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"),
            PathBuf::from("/Applications/Visual Studio Code.app/Contents/MacOS/Electron"),
        ];
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return Some(found);
        }
    }

    if let Some(found) = find_on_path("code.exe") {
        return Some(found);
    }

    // Fallback: if standalone Microsoft VS Code binary is not present,
    // Cursor and Antigravity provide compatible VS Code engines.
    if let Some(cursor) = find_cursor() {
        return Some(cursor);
    }
    if let Some(antigravity) = find_antigravity() {
        return Some(antigravity);
    }

    find_on_path("code")
}

fn find_tool_launcher(tool: ProjectTool) -> Option<PathBuf> {
    match tool {
        ProjectTool::VsCode => find_vscode(),
        ProjectTool::Cursor => find_cursor(),
        ProjectTool::Antigravity => find_antigravity(),
        ProjectTool::Explorer => {
            if cfg!(target_os = "windows") {
                Some(PathBuf::from("explorer.exe"))
            } else if cfg!(target_os = "macos") {
                Some(PathBuf::from("open"))
            } else {
                Some(PathBuf::from("xdg-open"))
            }
        }
    }
}

fn tool_rows() -> Vec<ToolAvailability> {
    [
        (ProjectTool::VsCode, "VS Code"),
        (ProjectTool::Cursor, "Cursor"),
        (ProjectTool::Antigravity, "Antigravity"),
    ]
    .into_iter()
    .map(|(tool, label)| {
        let available = find_tool_launcher(tool).is_some();
        ToolAvailability {
            tool,
            label: label.to_owned(),
            available,
            hint: if available {
                format!("Open this project in {label}.")
            } else {
                format!("Install {label} and add its command-line launcher to PATH.")
            },
        }
    })
    .chain(std::iter::once(ToolAvailability {
        tool: ProjectTool::Explorer,
        label: if cfg!(target_os = "macos") {
            "Finder".to_owned()
        } else {
            "File Explorer".to_owned()
        },
        available: true,
        hint: "Open the project folder in the system file manager.".to_owned(),
    }))
    .collect()
}

#[tauri::command]
#[specta::specta]
pub async fn project_tools() -> Result<Vec<ToolAvailability>, AppError> {
    Ok(tool_rows())
}

fn tool_command(tool: ProjectTool, path: &Path) -> Result<std::process::Command, AppError> {
    if tool == ProjectTool::Explorer {
        let mut command = if cfg!(target_os = "windows") {
            std::process::Command::new("explorer.exe")
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open")
        } else {
            std::process::Command::new("xdg-open")
        };
        command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        return Ok(command);
    }

    let launcher = find_tool_launcher(tool).ok_or_else(|| AppError {
        message: format!("{} is not available.", tool_label(tool)),
        hint: Some(format!(
            "Install {} and ensure its executable is in your programs or PATH.",
            tool_label(tool)
        )),
    })?;

    let is_batch = launcher
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);

    let mut command = if cfg!(target_os = "windows") && is_batch {
        let mut cmd = std::process::Command::new("cmd.exe");
        cmd.args(["/C", "call"]).arg(&launcher).arg(path);
        cmd
    } else {
        let mut cmd = std::process::Command::new(&launcher);
        cmd.arg(path);
        cmd
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(if is_batch {
            DETACHED_PROCESS | CREATE_NO_WINDOW
        } else {
            DETACHED_PROCESS
        });
    }

    Ok(command)
}

#[tauri::command]
#[specta::specta]
pub async fn open_project_in(path: String, tool: ProjectTool) -> Result<(), AppError> {
    let path = canonical_directory(&path)?;
    tool_command(tool, &path)?.spawn().map_err(|error| {
        AppError::plain(format!(
            "Could not open the project in {}: {error}",
            tool_label(tool)
        ))
    })?;
    Ok(())
}

fn tool_label(tool: ProjectTool) -> &'static str {
    match tool {
        ProjectTool::VsCode => "VS Code",
        ProjectTool::Cursor => "Cursor",
        ProjectTool::Antigravity => "Antigravity",
        ProjectTool::Explorer => "File Explorer",
    }
}

#[tauri::command]
#[specta::specta]
pub async fn initialize_project_git(path: String) -> Result<ProjectSummary, AppError> {
    let path = canonical_directory(&path)?;
    if path.join(".git").exists() {
        return Err(AppError::plain("This project is already a Git repository."));
    }
    let output = tokio::process::Command::new("git")
        .args(["-C", &display_path(&path), "init"])
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| AppError::plain(format!("Git could not start: {error}")))?;
    if !output.status.success() {
        return Err(AppError {
            message: "Git could not initialize this project.".to_owned(),
            hint: Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        });
    }
    Ok(ProjectSummary {
        name: project_name(&path),
        path: display_path(&path),
        is_git_repository: true,
        branch: branch_for(&path).await,
        active: true,
        last_opened_at: now_timestamp(),
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CliCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub success: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn run_cli_command(
    path: String,
    shell: String,
    command: String,
) -> Result<CliCommandResult, AppError> {
    let project_dir = canonical_directory(&path)?;
    let trimmed_cmd = command.trim();
    if trimmed_cmd.is_empty() {
        return Err(AppError::plain("Enter a command to run."));
    }

    let mut cmd = match shell.as_str() {
        "cmd" => {
            let mut c = tokio::process::Command::new("cmd.exe");
            c.args(["/c", trimmed_cmd]);
            c
        }
        "git_bash" | "bash" => {
            let bash_path = find_git_bash().unwrap_or_else(|| PathBuf::from("bash"));
            let mut c = tokio::process::Command::new(bash_path);
            c.args(["-c", trimmed_cmd]);
            c
        }
        "wsl" => {
            let mut c = tokio::process::Command::new("wsl.exe");
            c.args(["--", "bash", "-c", trimmed_cmd]);
            c
        }
        _ => {
            // Default to PowerShell on Windows, sh on Unix
            if cfg!(target_os = "windows") {
                let mut c = tokio::process::Command::new("powershell.exe");
                c.args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    trimmed_cmd,
                ]);
                c
            } else {
                let mut c = tokio::process::Command::new("sh");
                c.args(["-c", trimmed_cmd]);
                c
            }
        }
    };

    cmd.current_dir(&project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().await.map_err(|err| AppError {
        message: format!("Failed to execute command: {err}"),
        hint: Some(
            "Check that the selected shell is available and configured on your system.".to_owned(),
        ),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();
    let success = output.status.success();

    Ok(CliCommandResult {
        stdout,
        stderr,
        exit_code,
        success,
    })
}

fn find_git_bash() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("C:\\Program Files\\Git\\bin\\bash.exe"),
        PathBuf::from("C:\\Program Files (x86)\\Git\\bin\\bash.exe"),
        dirs_in_local_appdata().join("Programs\\Git\\bin\\bash.exe"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
fn find_git_bash_gui() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("C:\\Program Files\\Git\\git-bash.exe"),
        PathBuf::from("C:\\Program Files (x86)\\Git\\git-bash.exe"),
        dirs_in_local_appdata().join("Programs\\Git\\git-bash.exe"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

#[tauri::command]
#[specta::specta]
pub async fn open_external_terminal(
    path: String,
    shell: String,
    custom_cmd: Option<String>,
) -> Result<(), AppError> {
    let project_dir = canonical_directory(&path)?;
    let disp_path = display_path(&project_dir);

    #[cfg(not(windows))]
    let _ = (&shell, &custom_cmd);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;

        let mut command = match shell.as_str() {
            "cmd" => {
                let mut cmd = std::process::Command::new("cmd.exe");
                cmd.args(["/K", &format!("cd /d \"{}\"", disp_path)]);
                cmd
            }
            "git_bash" => {
                if let Some(git_bash) = find_git_bash_gui() {
                    let mut cmd = std::process::Command::new(git_bash);
                    cmd.arg(format!("--cd={}", disp_path));
                    cmd
                } else {
                    let mut cmd = std::process::Command::new("cmd.exe");
                    cmd.args(["/K", &format!("cd /d \"{}\"", disp_path)]);
                    cmd
                }
            }
            "wsl" => {
                let mut cmd = std::process::Command::new("wsl.exe");
                cmd.current_dir(&project_dir);
                cmd
            }
            "custom" if custom_cmd.is_some() => {
                let raw = custom_cmd.unwrap_or_default();
                let trimmed = raw.trim();
                let mut parts = trimmed.split_whitespace();
                let prog = parts.next().unwrap_or("powershell.exe");
                let mut cmd = std::process::Command::new(prog);
                for arg in parts {
                    cmd.arg(arg);
                }
                cmd.current_dir(&project_dir);
                cmd
            }
            _ => {
                // Default: Windows Terminal or PowerShell
                if executable_on_path("wt") {
                    let mut cmd = std::process::Command::new("wt.exe");
                    cmd.args(["-d", &disp_path]);
                    cmd
                } else {
                    let mut cmd = std::process::Command::new("powershell.exe");
                    cmd.args([
                        "-NoExit",
                        "-Command",
                        &format!("Set-Location -LiteralPath '{}'", disp_path),
                    ]);
                    cmd
                }
            }
        };

        command.creation_flags(DETACHED_PROCESS);
        command.spawn().map_err(|error| AppError {
            message: format!("Could not launch external terminal: {error}"),
            hint: Some("Verify that the terminal application exists on your system.".to_owned()),
        })?;
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        cmd.args(["-a", "Terminal", &disp_path]);
        cmd.spawn()
            .map_err(|error| AppError::plain(format!("Could not launch terminal: {error}")))?;
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let mut cmd = std::process::Command::new("x-terminal-emulator");
        cmd.arg(format!("--working-directory={}", disp_path));
        cmd.spawn()
            .map_err(|error| AppError::plain(format!("Could not launch terminal: {error}")))?;
    }

    Ok(())
}

fn http_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.len() > 2048 {
        return Err(AppError::plain("That address is too long to open."));
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(AppError::plain(
            "Only http and https addresses can be opened.",
        ));
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_control() || ch == '"' || ch == '<' || ch == '>')
    {
        return Err(AppError::plain(
            "That address contains characters that cannot be opened.",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Opens an http(s) address in the user's default browser. The in-app browser uses this
/// when a site must leave the pane (the dedicated-window buttons on a blocked card).
#[tauri::command]
#[specta::specta]
pub async fn open_external_url(url: String) -> Result<(), AppError> {
    let url = http_url(&url)?;
    let mut command = {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd.exe")
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            std::process::Command::new("xdg-open")
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command
            .args(["/c", "start", "", &url])
            .creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        command.arg(&url);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppError::plain(format!("could not open the browser: {error}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        http_url, strip_verbatim_prefix, tool_command, tool_rows, validate_git_url,
        validate_project_name, ProjectTool,
    };
    use std::path::Path;

    #[test]
    fn only_http_urls_open_in_the_system_browser() {
        assert!(http_url("https://www.google.com").is_ok());
        assert!(http_url("http://localhost:5173").is_ok());
        assert!(http_url("file:///etc/passwd").is_err());
        assert!(http_url("https://evil.example/\ncalc").is_err());
    }

    #[test]
    fn project_names_reject_path_traversal() {
        assert!(validate_project_name("../outside").is_err());
        assert!(validate_project_name("folder/name").is_err());
        assert_eq!(validate_project_name("ade-shell").ok(), Some("ade-shell"));
    }

    #[test]
    fn clone_only_accepts_explicit_git_transports() {
        assert!(validate_git_url("https://github.com/acme/app.git").is_ok());
        assert!(validate_git_url("git@github.com:acme/app.git").is_ok());
        assert!(validate_git_url("file:///private/repo").is_err());
        assert!(validate_git_url("--upload-pack=bad").is_err());
    }

    #[test]
    fn strip_verbatim_prefix_cleans_windows_extended_paths() {
        let p = Path::new(r"\\?\C:\Projects\bhippi");
        let cleaned = strip_verbatim_prefix(p);
        #[cfg(windows)]
        assert_eq!(cleaned, Path::new(r"C:\Projects\bhippi"));
        #[cfg(not(windows))]
        assert_eq!(cleaned, p);
    }

    #[test]
    fn tool_rows_lists_all_tools_and_explorer_is_always_available() {
        let rows = tool_rows();
        assert!(rows.iter().any(|r| r.tool == ProjectTool::VsCode));
        assert!(rows.iter().any(|r| r.tool == ProjectTool::Cursor));
        assert!(rows.iter().any(|r| r.tool == ProjectTool::Antigravity));
        let explorer = rows
            .iter()
            .find(|r| r.tool == ProjectTool::Explorer)
            .unwrap();
        assert!(explorer.available);
    }

    #[test]
    fn tool_command_builds_explorer_command() {
        let temp = std::env::temp_dir();
        let cmd = tool_command(ProjectTool::Explorer, &temp);
        assert!(cmd.is_ok());
    }

    #[test]
    fn test_detected_tools_output() {
        let rows = tool_rows();
        let temp = std::env::temp_dir();
        for r in &rows {
            println!(
                "TOOL: {:?} | available={} | hint={}",
                r.tool, r.available, r.hint
            );
            if r.available {
                let cmd = tool_command(r.tool, &temp);
                assert!(cmd.is_ok(), "tool_command for {:?} should succeed", r.tool);
                println!("  COMMAND: {:?}", cmd.unwrap());
            }
        }
        assert!(rows.iter().any(|r| r.available));
    }

    #[tokio::test]
    async fn run_cli_command_executes_in_directory() {
        let temp = std::env::temp_dir();
        let res = super::run_cli_command(
            temp.to_string_lossy().to_string(),
            "cmd".to_string(),
            "echo hello_bhippi".to_string(),
        )
        .await;
        assert!(res.is_ok());
        let output = res.unwrap();
        assert!(output.success);
        assert!(output.stdout.contains("hello_bhippi"));
    }
}
