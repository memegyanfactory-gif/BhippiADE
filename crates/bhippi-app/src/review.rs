//! Git review diff parsing and change inspection for project conversations and turns.

use crate::commands::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ReviewSummary {
    pub files: Vec<FileDiff>,
    pub total_additions: usize,
    pub total_deletions: usize,
    pub turn_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FileDiff {
    pub path: String,
    pub filename: String,
    pub directory: String,
    pub additions: usize,
    pub deletions: usize,
    pub status: String,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineType {
    Added,
    Deleted,
    Context,
}

pub async fn collect_review_changes(
    project_path: &Path,
    turn_title: Option<String>,
) -> Result<ReviewSummary, AppError> {
    let mut files = Vec::new();
    let mut total_additions = 0;
    let mut total_deletions = 0;

    let has_git = project_path.join(".git").exists();
    if has_git {
        // 1. Run git diff HEAD (working tree vs HEAD, including staged and unstaged)
        let diff_output = tokio::process::Command::new("git")
            .args(["diff", "-U3", "HEAD"])
            .current_dir(project_path)
            .output()
            .await;

        let diff_text = match diff_output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
            Err(_) => String::new(),
        };

        if !diff_text.trim().is_empty() {
            let parsed_files = parse_git_diff(&diff_text);
            for file in parsed_files {
                total_additions += file.additions;
                total_deletions += file.deletions;
                files.push(file);
            }
        }

        // 2. Also check untracked files from git status --porcelain
        let status_output = tokio::process::Command::new("git")
            .args(["status", "--porcelain=v1", "-u"])
            .current_dir(project_path)
            .output()
            .await;

        if let Ok(status) = status_output {
            let status_text = String::from_utf8_lossy(&status.stdout);
            for line in status_text.lines() {
                let trimmed = line.trim();
                if let Some(stripped) = trimmed.strip_prefix("??") {
                    let rel_path = stripped.trim().trim_matches('"');
                    let full_path = project_path.join(rel_path);
                    if full_path.is_file() {
                        if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                            let file_lines: Vec<&str> = content.lines().collect();
                            let additions = file_lines.len();
                            total_additions += additions;

                            let path_obj = Path::new(rel_path);
                            let filename = path_obj
                                .file_name()
                                .map(|f| f.to_string_lossy().into_owned())
                                .unwrap_or_else(|| rel_path.to_string());
                            let directory = path_obj
                                .parent()
                                .map(|p| p.to_string_lossy().replace('\\', "/"))
                                .filter(|p| !p.is_empty())
                                .unwrap_or_else(|| ".".to_string());

                            let diff_lines: Vec<DiffLine> = file_lines
                                .iter()
                                .enumerate()
                                .map(|(idx, line_str)| DiffLine {
                                    line_type: DiffLineType::Added,
                                    old_line_num: None,
                                    new_line_num: Some(idx + 1),
                                    content: (*line_str).to_owned(),
                                })
                                .collect();

                            let hunk = DiffHunk {
                                old_start: 0,
                                old_lines: 0,
                                new_start: 1,
                                new_lines: additions,
                                header: format!("@@ -0,0 +1,{} @@", additions),
                                lines: diff_lines,
                            };

                            files.push(FileDiff {
                                path: rel_path.replace('\\', "/"),
                                filename,
                                directory,
                                additions,
                                deletions: 0,
                                status: "added".to_owned(),
                                hunks: vec![hunk],
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(ReviewSummary {
        files,
        total_additions,
        total_deletions,
        turn_title,
    })
}

/// Parses git unified diff text output into structured `FileDiff` models.
pub fn parse_git_diff(raw_diff: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_hunks: Vec<DiffHunk> = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut current_old_line = 0;
    let mut current_new_line = 0;
    let mut additions = 0;
    let mut deletions = 0;

    let flush_file = |files: &mut Vec<FileDiff>,
                      path_opt: Option<String>,
                      mut hunks: Vec<DiffHunk>,
                      hunk_opt: Option<DiffHunk>,
                      additions: usize,
                      deletions: usize| {
        if let Some(hunk) = hunk_opt {
            hunks.push(hunk);
        }
        if let Some(path) = path_opt {
            let path_obj = Path::new(&path);
            let filename = path_obj
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            let directory = path_obj
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| ".".to_string());

            let status = if deletions == 0 && additions > 0 {
                "added".to_owned()
            } else if additions == 0 && deletions > 0 {
                "deleted".to_owned()
            } else {
                "modified".to_owned()
            };

            files.push(FileDiff {
                path,
                filename,
                directory,
                additions,
                deletions,
                status,
                hunks,
            });
        }
    };

    for line in raw_diff.lines() {
        if line.starts_with("diff --git ") {
            flush_file(
                &mut files,
                current_path.take(),
                std::mem::take(&mut current_hunks),
                current_hunk.take(),
                additions,
                deletions,
            );
            additions = 0;
            deletions = 0;

            // Extract file path from diff --git a/... b/...
            if let Some(b_idx) = line.rfind(" b/") {
                current_path = Some(line[b_idx + 3..].trim().to_owned());
            }
        } else if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("index ")
        {
            // Header metadata lines
        } else if line.starts_with("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                current_hunks.push(hunk);
            }
            // Parse @@ -A,B +C,D @@
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
            current_old_line = old_start;
            current_new_line = new_start;
            current_hunk = Some(DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                header: line.to_owned(),
                lines: Vec::new(),
            });
        } else if let Some(ref mut hunk) = current_hunk {
            if let Some(content) = line.strip_prefix('+') {
                additions += 1;
                hunk.lines.push(DiffLine {
                    line_type: DiffLineType::Added,
                    old_line_num: None,
                    new_line_num: Some(current_new_line),
                    content: content.to_owned(),
                });
                current_new_line += 1;
            } else if let Some(content) = line.strip_prefix('-') {
                deletions += 1;
                hunk.lines.push(DiffLine {
                    line_type: DiffLineType::Deleted,
                    old_line_num: Some(current_old_line),
                    new_line_num: None,
                    content: content.to_owned(),
                });
                current_old_line += 1;
            } else if let Some(content) = line.strip_prefix(' ') {
                hunk.lines.push(DiffLine {
                    line_type: DiffLineType::Context,
                    old_line_num: Some(current_old_line),
                    new_line_num: Some(current_new_line),
                    content: content.to_owned(),
                });
                current_old_line += 1;
                current_new_line += 1;
            } else if line.starts_with('\\') {
                // "\ No newline at end of file"
            }
        }
    }

    flush_file(
        &mut files,
        current_path,
        current_hunks,
        current_hunk,
        additions,
        deletions,
    );

    files
}

fn parse_hunk_header(line: &str) -> (usize, usize, usize, usize) {
    let mut parts = line.split("@@");
    let _ = parts.next();
    if let Some(range_part) = parts.next() {
        let ranges: Vec<&str> = range_part.split_whitespace().collect();
        let old_range = ranges.first().unwrap_or(&"-1");
        let new_range = ranges.get(1).unwrap_or(&"+1");

        let parse_range = |s: &str| -> (usize, usize) {
            let cleaned = s.trim_start_matches('-').trim_start_matches('+');
            if let Some((start, count)) = cleaned.split_once(',') {
                (
                    start.parse::<usize>().unwrap_or(1),
                    count.parse::<usize>().unwrap_or(1),
                )
            } else {
                (cleaned.parse::<usize>().unwrap_or(1), 1)
            }
        };

        let (old_start, old_lines) = parse_range(old_range);
        let (new_start, new_lines) = parse_range(new_range);
        (old_start, old_lines, new_start, new_lines)
    } else {
        (1, 1, 1, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_file_unified_diff() {
        let raw = r#"diff --git a/crates/bhippi-core/src/config.rs b/crates/bhippi-core/src/config.rs
index 123456..789abc 100644
--- a/crates/bhippi-core/src/config.rs
+++ b/crates/bhippi-core/src/config.rs
@@ -1,4 +1,4 @@
 use std::path::PathBuf;
-use old_lib;
+use new_lib;
 pub struct Config;
diff --git a/docs/PROGRESS.md b/docs/PROGRESS.md
index 111111..222222 100644
--- a/docs/PROGRESS.md
+++ b/docs/PROGRESS.md
@@ -10,3 +10,4 @@
 line 10
+line 11 added
 line 12
"#;

        let files = parse_git_diff(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "crates/bhippi-core/src/config.rs");
        assert_eq!(files[0].filename, "config.rs");
        assert_eq!(files[0].directory, "crates/bhippi-core/src");
        assert_eq!(files[0].additions, 1);
        assert_eq!(files[0].deletions, 1);

        assert_eq!(files[1].path, "docs/PROGRESS.md");
        assert_eq!(files[1].filename, "PROGRESS.md");
        assert_eq!(files[1].directory, "docs");
        assert_eq!(files[1].additions, 1);
        assert_eq!(files[1].deletions, 0);
    }
}
