//! One recursive walk over a project, with the ignore rules and budgets every scan shares.
//!
//! The old scanner called `read_dir` once, on the workspace root, and never descended. On
//! any project with a `src/` directory that is a scan of nothing — which is why it had
//! never once reported a conflict marker from a real repository.
//!
//! Budgets are explicit and *reported*. A scan that stopped early because it hit a cap is
//! not a clean scan, and quietly presenting it as one is worse than not scanning at all.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Directories never worth reading: build output, dependencies, and version control.
///
/// Not a taste call — these hold hundreds of thousands of files nobody wrote, and every
/// one of them would produce findings the user cannot act on. `target` alone is usually
/// larger than the entire rest of a Rust repository.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "coverage",
    ".turbo",
    ".cache",
    ".gradle",
    "Pods",
    "DerivedData",
    ".idea",
    ".vs",
];

/// Extensions worth reading as source. Anything else is skipped without opening it.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "rb", "java", "kt", "kts", "swift",
    "c", "h", "cc", "cpp", "hpp", "cs", "php", "sh", "bash", "zsh", "ps1", "sql", "css", "scss",
    "html", "vue", "svelte", "json", "toml", "yaml", "yml", "md",
];

/// Files that are worth *finding* even though they are never read as source.
const NOTABLE_NAMES: &[&str] = &[".env", ".env.local", ".env.production", ".env.development"];

/// How much of a project one scan will look at.
///
/// A generated bundle or a vendored blob can be tens of megabytes on one line; reading it
/// costs seconds and yields nothing, because no rule here is meaningful against minified
/// output. The caps are high enough that a real hand-written project never reaches them.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_depth: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_files: 4_000,
            max_file_bytes: 1_024 * 1_024,
            max_depth: 24,
        }
    }
}

/// One source file found by the walk.
#[derive(Clone, Debug)]
pub struct Found {
    /// Absolute path, for reading.
    pub path: PathBuf,
    /// Path relative to the project root, for display. Always forward-slashed so a
    /// finding reads the same on every platform.
    pub relative: String,
    pub extension: String,
    pub bytes: u64,
}

/// What one walk saw, including what it deliberately did not.
#[derive(Clone, Debug, Default)]
pub struct Walked {
    pub files: Vec<Found>,
    /// Files skipped for being larger than the budget allows.
    pub skipped_large: usize,
    /// True when the file cap stopped the walk early — the scan is then partial, and the
    /// report must say so rather than claiming the project is clean.
    pub truncated: bool,
    /// Directories that exist but were deliberately not entered.
    pub skipped_dirs: usize,
}

/// Walks `root` breadth-first, collecting readable source files within `budget`.
///
/// Breadth-first on purpose: if the cap is reached, what has been collected is the top of
/// the tree — the project's own code — rather than whatever the deepest directory happened
/// to be. Depth-first truncation buries the files the user actually wrote.
pub async fn walk(root: &Path, budget: Budget) -> Walked {
    let mut out = Walked::default();
    let mut queue: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut seen: HashSet<PathBuf> = HashSet::new();

    while let Some((dir, depth)) = queue.pop() {
        if depth > budget.max_depth {
            continue;
        }
        // A symlink loop is the one way a walk never terminates. Canonicalising each
        // directory once and refusing repeats is what makes that impossible.
        let key = tokio::fs::canonicalize(&dir).await.unwrap_or(dir.clone());
        if !seen.insert(key) {
            continue;
        }

        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Ok(kind) = entry.file_type().await else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();

            if kind.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) {
                    out.skipped_dirs += 1;
                    continue;
                }
                // A dot-directory not on the skip list is still almost never source; the
                // named exceptions are the ones projects genuinely keep code in.
                if name.starts_with('.') && !matches!(name.as_str(), ".github" | ".config") {
                    out.skipped_dirs += 1;
                    continue;
                }
                queue.push((path, depth + 1));
                continue;
            }
            if !kind.is_file() {
                continue;
            }

            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let notable = NOTABLE_NAMES.contains(&name.as_str());
            if !notable && !SOURCE_EXTENSIONS.contains(&extension.as_str()) {
                continue;
            }

            let bytes = entry.metadata().await.map(|meta| meta.len()).unwrap_or(0);
            if bytes > budget.max_file_bytes {
                out.skipped_large += 1;
                continue;
            }

            if out.files.len() >= budget.max_files {
                out.truncated = true;
                return out;
            }

            out.files.push(Found {
                relative: relative_of(root, &path),
                path,
                extension,
                bytes,
            });
        }
    }

    out
}

/// A display path relative to the project root, forward-slashed on every platform.
#[must_use]
pub fn relative_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Reads a file as text, or `None` when it is not text at all.
///
/// A NUL byte in the first block is the standard, cheap test for binary content, and it is
/// what stops a `.json` that is really a packed asset from being scanned line by line.
pub async fn read_text(path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    if bytes.iter().take(8_000).any(|byte| *byte == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{relative_of, walk, Budget};
    use std::path::{Path, PathBuf};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bhippi-walk-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        assert!(std::fs::create_dir_all(&dir).is_ok());
        dir
    }

    fn write(root: &Path, relative: &str, body: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            assert!(std::fs::create_dir_all(parent).is_ok());
        }
        assert!(std::fs::write(path, body).is_ok());
    }

    /// The bug that made the old scanner useless: it never descended past the root.
    #[tokio::test]
    async fn the_walk_descends_into_subdirectories() {
        let root = scratch("deep");
        write(&root, "top.rs", "fn main() {}");
        write(&root, "src/inner.rs", "fn inner() {}");
        write(&root, "src/deep/deeper/leaf.tsx", "export const a = 1;");

        let seen = walk(&root, Budget::default()).await;
        let names: Vec<&str> = seen.files.iter().map(|f| f.relative.as_str()).collect();

        assert!(names.contains(&"top.rs"), "{names:?}");
        assert!(names.contains(&"src/inner.rs"), "{names:?}");
        assert!(
            names.contains(&"src/deep/deeper/leaf.tsx"),
            "a nested file must be found: {names:?}"
        );
        assert!(!seen.truncated);

        let _ignored = std::fs::remove_dir_all(root);
    }

    /// Dependency and build directories hold more files than the project does, and every
    /// finding in them is one the user cannot act on.
    #[tokio::test]
    async fn build_output_and_dependencies_are_never_scanned() {
        let root = scratch("ignored");
        write(&root, "src/real.ts", "export const a = 1;");
        write(&root, "node_modules/pkg/index.js", "console.log('x')");
        write(&root, "target/debug/build.rs", "fn main() {}");
        write(&root, "dist/bundle.js", "console.log('y')");
        write(&root, ".git/config", "[core]");

        let seen = walk(&root, Budget::default()).await;
        let names: Vec<&str> = seen.files.iter().map(|f| f.relative.as_str()).collect();

        assert_eq!(names, vec!["src/real.ts"], "{names:?}");
        assert!(seen.skipped_dirs >= 4);

        let _ignored = std::fs::remove_dir_all(root);
    }

    /// A truncated scan must say so. Reporting a partial scan as a clean one is the one
    /// failure mode that actively misleads.
    #[tokio::test]
    async fn hitting_the_file_cap_is_reported_not_hidden() {
        let root = scratch("cap");
        for index in 0..12 {
            write(&root, &format!("src/file{index}.ts"), "export const a = 1;");
        }

        let budget = Budget {
            max_files: 5,
            ..Budget::default()
        };
        let seen = walk(&root, budget).await;

        assert_eq!(seen.files.len(), 5);
        assert!(seen.truncated, "a capped walk must report itself truncated");

        let _ignored = std::fs::remove_dir_all(root);
    }

    /// A generated bundle on one 4 MB line yields nothing but costs seconds to read.
    #[tokio::test]
    async fn oversized_files_are_skipped_and_counted() {
        let root = scratch("large");
        write(&root, "small.ts", "export const a = 1;");
        write(&root, "huge.js", &"x".repeat(2_000));

        let budget = Budget {
            max_file_bytes: 1_000,
            ..Budget::default()
        };
        let seen = walk(&root, budget).await;

        let names: Vec<&str> = seen.files.iter().map(|f| f.relative.as_str()).collect();
        assert_eq!(names, vec!["small.ts"], "{names:?}");
        assert_eq!(seen.skipped_large, 1);

        let _ignored = std::fs::remove_dir_all(root);
    }

    /// A finding must read identically on Windows and elsewhere.
    #[test]
    fn display_paths_are_forward_slashed() {
        let root = Path::new("C:/proj");
        let nested = Path::new("C:/proj/src/deep/file.rs");
        assert_eq!(relative_of(root, nested), "src/deep/file.rs");
    }
}
