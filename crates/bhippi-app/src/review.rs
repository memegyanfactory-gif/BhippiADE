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

/// Lines of unchanged text kept either side of a change, as `git diff` shows them.
const DIFF_CONTEXT: usize = 3;

/// The largest file pair compared line by line.
///
/// The comparison is a quadratic longest-common-subsequence, which is the right shape for
/// the game scripts and scenes this actually sees. Past this, a precise diff would cost more
/// memory than the answer is worth, so the file is reported as replaced -- true, and honest
/// about being coarse.
const DIFF_LINE_CAP: usize = 2_000;

/// What changed in this workspace, and by how much.
///
/// Three sources, in order of how much they know:
///
/// 1. **The review ledger.** Every write path records what a file held before Bhippi first
///    touched it, so this is the real diff of the work -- additions *and* deletions -- and
///    it does not care whether the project is under version control. Most games are not.
/// 2. **Git**, when the workspace is a repository with a commit to compare against. It
///    catches what a person changed by hand, which the ledger never sees.
/// 3. **The project itself**, when neither of the above knows anything. A game Bhippi built
///    with no recorded history is all new work, so every line present is reported as added.
///
/// The ledger wins per path: it knows what *this session* did, which is the question the
/// panel is actually asking.
pub async fn collect_review_changes(
    project_path: &Path,
    turn_title: Option<String>,
) -> Result<ReviewSummary, AppError> {
    let mut files: Vec<FileDiff> = Vec::new();

    for baseline in crate::engine::review_baselines(project_path).await {
        if let Some(diff) = diff_against_disk(&baseline).await {
            files.push(diff);
        }
    }

    if has_commit(project_path).await {
        collect_from_git(project_path, &mut files).await;
    } else if files.is_empty() {
        scan_project_files(project_path, project_path, &mut files).await;
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let total_additions = files.iter().map(|file| file.additions).sum();
    let total_deletions = files.iter().map(|file| file.deletions).sum();

    Ok(ReviewSummary {
        files,
        total_additions,
        total_deletions,
        turn_title,
    })
}

/// True when this workspace is a git repository that has something to diff against.
///
/// A repository with no commit has no `HEAD`, so `git diff HEAD` fails and every file reads
/// as untracked. Asking first is what keeps a fresh `git init` from being reported as a
/// project-sized addition.
async fn has_commit(project_path: &Path) -> bool {
    if !project_path.join(".git").exists() {
        return false;
    }
    tokio::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(project_path)
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Fold git's own view of the workspace in, for every path the ledger did not already claim.
async fn collect_from_git(project_path: &Path, files: &mut Vec<FileDiff>) {
    let diff_text = tokio::process::Command::new("git")
        .args(["diff", "-U3", "HEAD"])
        .current_dir(project_path)
        .output()
        .await
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();

    if !diff_text.trim().is_empty() {
        for file in parse_git_diff(&diff_text) {
            if !files.iter().any(|kept| kept.path == file.path) {
                files.push(file);
            }
        }
    }

    let Ok(status) = tokio::process::Command::new("git")
        .args(["status", "--porcelain=v1", "-u"])
        .current_dir(project_path)
        .output()
        .await
    else {
        return;
    };

    let status_text = String::from_utf8_lossy(&status.stdout);
    for line in status_text.lines() {
        let trimmed = line.trim();
        let Some(rel_path) = trimmed
            .strip_prefix("??")
            .or_else(|| trimmed.strip_prefix("A "))
            .map(|rest| rest.trim().trim_matches('"'))
        else {
            continue;
        };
        let normalised = rel_path.replace('\\', "/");
        if files.iter().any(|kept| kept.path == normalised) {
            continue;
        }
        let full_path = project_path.join(rel_path);
        if !full_path.is_file() {
            continue;
        }
        if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
            files.push(whole_file_diff(&normalised, &content, DiffLineType::Added));
        }
    }
}

/// One ledger row, compared against what is on disk now.
///
/// `None` means nothing to report: a file that was created and then removed again, or one
/// that has been put back exactly as it was.
async fn diff_against_disk(baseline: &bhippi_db::ReviewBaseline) -> Option<FileDiff> {
    let current = tokio::fs::read_to_string(Path::new(&baseline.file_path))
        .await
        .ok();
    let rel = baseline.rel_path.as_str();

    match (baseline.existed, baseline.before_text.as_deref(), current) {
        // Bhippi made this file. Every line in it is new work.
        (false, _, Some(after)) => Some(whole_file_diff(rel, &after, DiffLineType::Added)),
        (false, _, None) => None,
        // It was here, and now it is not.
        (true, Some(before), None) => Some(whole_file_diff(rel, before, DiffLineType::Deleted)),
        (true, Some(before), Some(after)) if before != after => {
            Some(text_diff(rel, before, &after))
        }
        (true, Some(_), Some(_)) => None,
        // Too large to have kept a copy of, so it can be named but not counted. Inventing a
        // number here would be worse than admitting the file is only known to have changed.
        (true, None, current) => Some(FileDiff {
            path: rel.to_owned(),
            filename: name_of(rel),
            directory: directory_of(rel),
            additions: 0,
            deletions: 0,
            status: if current.is_some() {
                "modified"
            } else {
                "deleted"
            }
            .to_owned(),
            hunks: Vec::new(),
        }),
    }
}

fn name_of(rel: &str) -> String {
    Path::new(rel)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_owned())
}

fn directory_of(rel: &str) -> String {
    Path::new(rel)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| !parent.is_empty())
        .unwrap_or_else(|| ".".to_owned())
}

/// A file that is entirely new or entirely gone, as one hunk.
fn whole_file_diff(rel: &str, text: &str, kind: DiffLineType) -> FileDiff {
    let text_lines: Vec<&str> = text.lines().collect();
    let count = text_lines.len();
    let added = matches!(kind, DiffLineType::Added);

    let lines: Vec<DiffLine> = text_lines
        .iter()
        .enumerate()
        .map(|(index, line)| DiffLine {
            line_type: kind,
            old_line_num: (!added).then_some(index + 1),
            new_line_num: added.then_some(index + 1),
            content: (*line).to_owned(),
        })
        .collect();

    let header = if added {
        format!("@@ -0,0 +1,{count} @@")
    } else {
        format!("@@ -1,{count} +0,0 @@")
    };

    FileDiff {
        path: rel.to_owned(),
        filename: name_of(rel),
        directory: directory_of(rel),
        additions: if added { count } else { 0 },
        deletions: if added { 0 } else { count },
        status: if added { "added" } else { "deleted" }.to_owned(),
        hunks: vec![DiffHunk {
            old_start: if added { 0 } else { 1 },
            old_lines: if added { 0 } else { count },
            new_start: if added { 1 } else { 0 },
            new_lines: if added { count } else { 0 },
            header,
            lines,
        }],
    }
}

/// The real diff of two versions of one file: a longest-common-subsequence over the lines,
/// cut into hunks with context, the way `git diff` presents it.
fn text_diff(rel: &str, before: &str, after: &str) -> FileDiff {
    let old_lines: Vec<&str> = before.lines().collect();
    let new_lines: Vec<&str> = after.lines().collect();

    let script = if old_lines.len() > DIFF_LINE_CAP || new_lines.len() > DIFF_LINE_CAP {
        replaced_script(&old_lines, &new_lines)
    } else {
        lcs_script(&old_lines, &new_lines)
    };

    let additions = script
        .iter()
        .filter(|line| line.line_type == DiffLineType::Added)
        .count();
    let deletions = script
        .iter()
        .filter(|line| line.line_type == DiffLineType::Deleted)
        .count();

    FileDiff {
        path: rel.to_owned(),
        filename: name_of(rel),
        directory: directory_of(rel),
        additions,
        deletions,
        status: "modified".to_owned(),
        hunks: hunks_of(&script),
    }
}

/// Every old line out, every new line in -- the answer for a file too big to compare line by
/// line. The counts are real; only the grouping is coarse.
fn replaced_script(old_lines: &[&str], new_lines: &[&str]) -> Vec<DiffLine> {
    let mut script = Vec::with_capacity(old_lines.len() + new_lines.len());
    for (index, line) in old_lines.iter().enumerate() {
        script.push(DiffLine {
            line_type: DiffLineType::Deleted,
            old_line_num: Some(index + 1),
            new_line_num: None,
            content: (*line).to_owned(),
        });
    }
    for (index, line) in new_lines.iter().enumerate() {
        script.push(DiffLine {
            line_type: DiffLineType::Added,
            old_line_num: None,
            new_line_num: Some(index + 1),
            content: (*line).to_owned(),
        });
    }
    script
}

/// The line-by-line edit script, oldest position first.
fn lcs_script(old_lines: &[&str], new_lines: &[&str]) -> Vec<DiffLine> {
    let (rows, cols) = (old_lines.len(), new_lines.len());
    // `table[i][j]` is the length of the longest common subsequence of the two suffixes, so
    // walking forward from the origin and taking the better branch reads the script off in
    // order. `u32` because a file past `DIFF_LINE_CAP` never reaches here.
    let mut table = vec![0u32; (rows + 1) * (cols + 1)];
    for i in (0..rows).rev() {
        for j in (0..cols).rev() {
            table[i * (cols + 1) + j] = if old_lines[i] == new_lines[j] {
                table[(i + 1) * (cols + 1) + j + 1] + 1
            } else {
                table[(i + 1) * (cols + 1) + j].max(table[i * (cols + 1) + j + 1])
            };
        }
    }

    let mut script = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < rows && j < cols {
        if old_lines[i] == new_lines[j] {
            script.push(DiffLine {
                line_type: DiffLineType::Context,
                old_line_num: Some(i + 1),
                new_line_num: Some(j + 1),
                content: old_lines[i].to_owned(),
            });
            i += 1;
            j += 1;
        } else if table[(i + 1) * (cols + 1) + j] >= table[i * (cols + 1) + j + 1] {
            script.push(DiffLine {
                line_type: DiffLineType::Deleted,
                old_line_num: Some(i + 1),
                new_line_num: None,
                content: old_lines[i].to_owned(),
            });
            i += 1;
        } else {
            script.push(DiffLine {
                line_type: DiffLineType::Added,
                old_line_num: None,
                new_line_num: Some(j + 1),
                content: new_lines[j].to_owned(),
            });
            j += 1;
        }
    }
    while i < rows {
        script.push(DiffLine {
            line_type: DiffLineType::Deleted,
            old_line_num: Some(i + 1),
            new_line_num: None,
            content: old_lines[i].to_owned(),
        });
        i += 1;
    }
    while j < cols {
        script.push(DiffLine {
            line_type: DiffLineType::Added,
            old_line_num: None,
            new_line_num: Some(j + 1),
            content: new_lines[j].to_owned(),
        });
        j += 1;
    }
    script
}

/// Cut an edit script into hunks: runs of change, padded with `DIFF_CONTEXT` unchanged lines
/// either side, with runs that touch merged into one.
fn hunks_of(script: &[DiffLine]) -> Vec<DiffHunk> {
    let changed: Vec<usize> = script
        .iter()
        .enumerate()
        .filter(|(_, line)| line.line_type != DiffLineType::Context)
        .map(|(index, _)| index)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }

    let mut spans: Vec<(usize, usize)> = Vec::new();
    for index in changed {
        let from = index.saturating_sub(DIFF_CONTEXT);
        let to = (index + DIFF_CONTEXT + 1).min(script.len());
        match spans.last_mut() {
            // `<=` so two runs separated by exactly their context join, instead of repeating
            // the same lines in two hunks.
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => spans.push((from, to)),
        }
    }

    spans
        .into_iter()
        .map(|(from, to)| {
            let lines: Vec<DiffLine> = script[from..to].to_vec();
            let old_start = lines.iter().find_map(|line| line.old_line_num).unwrap_or(0);
            let new_start = lines.iter().find_map(|line| line.new_line_num).unwrap_or(0);
            let old_lines = lines
                .iter()
                .filter(|line| line.old_line_num.is_some())
                .count();
            let new_lines = lines
                .iter()
                .filter(|line| line.new_line_num.is_some())
                .count();
            DiffHunk {
                old_start,
                old_lines,
                new_start,
                new_lines,
                header: format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
                lines,
            }
        })
        .collect()
}

fn is_ignored_entry(name: &str) -> bool {
    name == ".git"
        || name == ".godot"
        || name == ".bhippi"
        || name == "target"
        || name == "node_modules"
        || name.ends_with(".import")
        || name.ends_with(".uid")
}

fn is_reviewable_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "gd" | "tscn" | "tres" | "godot" | "toml" | "json" | "md" | "txt" | "cfg" | "yaml" | "yml"
    )
}

/// Every reviewable file in the project, reported as new work.
///
/// The last resort, and only when nothing else knows anything: no ledger rows and no commit
/// to diff against. A game Bhippi built under those conditions genuinely is all new, so
/// every line present is an addition. Nothing here can report a deletion -- that is what the
/// ledger is for, and once it has a row for a file this is not consulted for it.
async fn scan_project_files(root: &Path, current_dir: &Path, files: &mut Vec<FileDiff>) {
    let Ok(mut entries) = tokio::fs::read_dir(current_dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_entry(&name) {
            continue;
        }
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if file_type.is_dir() {
            Box::pin(scan_project_files(root, &path, files)).await;
            continue;
        }
        if !file_type.is_file() || !is_reviewable_file(&path) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        if files.iter().any(|kept| kept.path == rel_path) {
            continue;
        }
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            files.push(whole_file_diff(&rel_path, &content, DiffLineType::Added));
        }
    }
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

    /// The number the owner actually reads. A whole-file rewrite used to be the only thing
    /// the panel could describe, so an edit that swapped one line and deleted another either
    /// vanished or was reported as the entire file.
    #[test]
    fn an_edited_file_reports_the_lines_that_moved_and_no_others() {
        let before = "one\ntwo\nthree\nfour\nfive\n";
        let after = "one\ntwo changed\nthree\nfive\n";

        let diff = text_diff("scripts/game.gd", before, after);

        assert_eq!(diff.status, "modified");
        assert_eq!(diff.additions, 1, "only the rewritten line is new");
        assert_eq!(diff.deletions, 2, "the rewritten line and the dropped one");
        assert_eq!(diff.filename, "game.gd");
        assert_eq!(diff.directory, "scripts");

        let lines: Vec<&DiffLine> = diff
            .hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .collect();
        let added: Vec<&str> = lines
            .iter()
            .filter(|line| line.line_type == DiffLineType::Added)
            .map(|line| line.content.as_str())
            .collect();
        let removed: Vec<&str> = lines
            .iter()
            .filter(|line| line.line_type == DiffLineType::Deleted)
            .map(|line| line.content.as_str())
            .collect();
        assert_eq!(added, vec!["two changed"]);
        assert_eq!(removed, vec!["two", "four"]);
    }

    /// A file that is gone is the case git could report and the old scan never could: the
    /// scan walks what exists, and a deleted file does not.
    #[test]
    fn a_deleted_file_counts_every_line_it_had_as_a_deletion() {
        let diff = whole_file_diff("scenes/old.tscn", "a\nb\nc\n", DiffLineType::Deleted);

        assert_eq!(diff.status, "deleted");
        assert_eq!(diff.additions, 0);
        assert_eq!(diff.deletions, 3);
        assert_eq!(diff.hunks.len(), 1);
        assert!(diff.hunks[0]
            .lines
            .iter()
            .all(|line| line.line_type == DiffLineType::Deleted));
        assert_eq!(diff.hunks[0].lines[0].old_line_num, Some(1));
        assert_eq!(diff.hunks[0].lines[0].new_line_num, None);
    }

    /// Two edits far apart are two hunks; two edits close together are one. Repeating the
    /// same context lines in adjacent hunks is how a diff view starts showing a line twice.
    #[test]
    fn hunks_split_on_distance_and_merge_when_their_context_touches() {
        let mut before = String::new();
        for index in 0..40 {
            before.push_str(&format!("line {index}\n"));
        }
        let far = before
            .replace("line 2\n", "line 2 edited\n")
            .replace("line 30\n", "line 30 edited\n");
        assert_eq!(text_diff("a.txt", &before, &far).hunks.len(), 2);

        let near = before
            .replace("line 10\n", "line 10 edited\n")
            .replace("line 12\n", "line 12 edited\n");
        assert_eq!(
            text_diff("a.txt", &before, &near).hunks.len(),
            1,
            "changes two lines apart share one hunk"
        );
    }

    /// An identical file is not a change, and a review that lists it is a review nobody can
    /// skim. The ledger keeps a row for every file Bhippi ever touched, so this is the guard
    /// that stops a reverted edit from showing up forever.
    #[test]
    fn a_file_put_back_as_it_was_reports_nothing() {
        let same = "unchanged\n";
        let diff = text_diff("a.txt", same, same);
        assert_eq!(diff.additions, 0);
        assert_eq!(diff.deletions, 0);
        assert!(diff.hunks.is_empty(), "no change means no hunk to draw");
    }

    /// The whole point, end to end: a game that is **not** a git repository, changed by
    /// Bhippi, reports what changed -- including the file that is no longer there.
    ///
    /// This is the defect the owner reported. A whole game was built and Review Changes said
    /// "No workspace changes", because the only thing it could read was `git diff` and a
    /// game Bhippi builds is usually not a repository. Scanning the folder instead cannot
    /// fix it either: a scan walks what exists, so a deletion is invisible to it and every
    /// surviving file reads as an addition forever.
    #[tokio::test]
    async fn a_non_git_game_reports_edits_creations_and_deletions_from_the_ledger() {
        let root = std::env::temp_dir().join(format!("bhippi-review-ledger-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(root.join("scripts")).expect("project directory");

        // `register_journal_db` is a `OnceLock`, so this test shares whichever database the
        // binary registered first. That is safe: the ledger is keyed by project path, and
        // this project path is unique to this run.
        let db_path =
            std::env::temp_dir().join(format!("bhippi-review-db-{}.db", ulid::Ulid::new()));
        let database = bhippi_db::Database::connect(&db_path)
            .await
            .expect("a temp review database");
        crate::engine::register_journal_db(database);

        // An existing script the turn is about to edit.
        let edited = root.join("scripts/player.gd");
        std::fs::write(
            &edited,
            "extends Node\nvar speed = 100\nfunc _ready():\n\tpass\n",
        )
        .expect("seed the edited file");
        crate::engine::record_review_baseline(
            &root,
            &edited,
            "scripts/player.gd",
            Some("extends Node\nvar speed = 100\nfunc _ready():\n\tpass\n"),
        )
        .await;
        std::fs::write(
            &edited,
            "extends Node\nvar speed = 250\nfunc _ready():\n\tpass\n",
        )
        .expect("the turn's edit");

        // A scene the turn creates.
        let created = root.join("scripts/enemy.gd");
        crate::engine::record_review_baseline(&root, &created, "scripts/enemy.gd", None).await;
        std::fs::write(&created, "extends Node\nfunc hit():\n\tpass\n")
            .expect("the turn's new file");

        // A script the turn deletes.
        let removed = root.join("scripts/old.gd");
        crate::engine::record_review_baseline(&root, &removed, "scripts/old.gd", Some("a\nb\n"))
            .await;

        let summary = collect_review_changes(&root, None)
            .await
            .expect("the review must collect");

        let by_path = |name: &str| {
            summary
                .files
                .iter()
                .find(|file| file.path == name)
                .unwrap_or_else(|| panic!("{name} must be in the review"))
        };

        assert_eq!(summary.files.len(), 3, "one row per file the turn touched");

        let edit = by_path("scripts/player.gd");
        assert_eq!(edit.status, "modified");
        assert_eq!(edit.additions, 1, "one line was rewritten");
        assert_eq!(edit.deletions, 1);

        let creation = by_path("scripts/enemy.gd");
        assert_eq!(creation.status, "added");
        assert_eq!(creation.additions, 3);
        assert_eq!(creation.deletions, 0);

        let deletion = by_path("scripts/old.gd");
        assert_eq!(deletion.status, "deleted");
        assert_eq!(deletion.additions, 0);
        assert_eq!(
            deletion.deletions, 2,
            "a deleted file's lines are deletions"
        );

        assert_eq!(summary.total_additions, 4);
        assert_eq!(summary.total_deletions, 3);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn collect_review_changes_on_non_git_dir_discovers_files() {
        let path = std::env::temp_dir().join(format!("bhippi-review-test-{}", ulid::Ulid::new()));
        let _ = std::fs::create_dir_all(&path);
        std::fs::write(
            path.join("main.gd"),
            "extends Node\nfunc _ready():\n\tpass\n",
        )
        .expect("write");
        std::fs::create_dir(path.join("scenes")).expect("mkdir");
        std::fs::write(
            path.join("scenes/main.tscn"),
            "[gd_scene format=3]\n[node name=\"Main\" type=\"Node\"]\n",
        )
        .expect("write");

        let summary = collect_review_changes(&path, None).await.expect("collect");
        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.total_additions, 5);
        assert_eq!(summary.total_deletions, 0);

        let _ = std::fs::remove_dir_all(&path);
    }
}
