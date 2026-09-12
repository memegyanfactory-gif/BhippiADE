//! The workspace file vocabulary advertised to every chat provider.
use std::path::{Path, PathBuf};

pub enum FileRequest {
    Read { path: String },
    Write { path: String, content: String },
}

impl FileRequest {
    pub fn path(&self) -> &str {
        match self {
            Self::Read { path } | Self::Write { path, .. } => path,
        }
    }
}

/// Remove only file protocol blocks, preserving other tools and ordinary prose.
pub fn parse(text: &str) -> (String, Vec<Result<FileRequest, String>>) {
    let mut visible = String::new();
    let mut requests = Vec::new();
    let mut cursor = 0;
    while let Some((offset, name)) = ["read_file", "write_file"]
        .into_iter()
        .filter_map(|name| {
            text[cursor..]
                .find(&format!("<{name}"))
                .map(|offset| (offset, name))
        })
        .min_by_key(|(offset, _)| *offset)
    {
        let start = cursor + offset;
        let after_name = start + name.len() + 1;
        if !text[after_name..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            visible.push_str(&text[cursor..after_name]);
            cursor = after_name;
            continue;
        }
        visible.push_str(&text[cursor..start]);
        let Some(end) = text[after_name..].find('>').map(|end| after_name + end) else {
            requests.push(Err(format!(
                "Incomplete <{name}> request; nothing was executed."
            )));
            return (visible, requests);
        };
        let header = text[after_name..end].trim();
        let path = path_attribute(header.trim_end_matches('/').trim());
        cursor = end + 1;
        let request = if name == "read_file" {
            if header.ends_with('/') {
                path.map(|path| FileRequest::Read { path })
            } else {
                Err("A read_file request must end with />.".to_owned())
            }
        } else {
            let Some(close) = text[cursor..].find("</write_file>") else {
                requests.push(Err(
                    "Incomplete <write_file> request; no file was written.".to_owned()
                ));
                return (visible, requests);
            };
            let body = &text[cursor..cursor + close];
            let content = body
                .strip_prefix("\r\n")
                .or_else(|| body.strip_prefix('\n'))
                .unwrap_or(body)
                .to_owned();
            cursor += close + "</write_file>".len();
            path.map(|path| FileRequest::Write { path, content })
        };
        requests.push(request);
    }
    visible.push_str(&text[cursor..]);
    (visible, requests)
}

fn path_attribute(header: &str) -> Result<String, String> {
    let invalid =
        || "File tools require one quoted path attribute relative to the workspace.".to_owned();
    let value = header
        .strip_prefix("path")
        .ok_or_else(invalid)?
        .trim_start()
        .strip_prefix('=')
        .ok_or_else(invalid)?
        .trim_start();
    let quote = value
        .chars()
        .next()
        .filter(|c| *c == '\'' || *c == '"')
        .ok_or_else(invalid)?;
    let tail = &value[1..];
    let end = tail.find(quote).ok_or_else(invalid)?;
    if !tail[end + 1..].trim().is_empty() || tail[..end].trim().is_empty() {
        return Err(invalid());
    }
    Ok(tail[..end].to_owned())
}

/// Resolve existing targets and the nearest existing ancestor before creating anything.
/// Junctions and symbolic links must resolve inside the project too.
pub async fn resolve(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = relative.trim().replace('\\', "/");
    if normalized.starts_with('/')
        || normalized
            .split('/')
            .any(|part| part == ".." || part.contains(':') || part.chars().any(char::is_control))
    {
        return Err(
            "That path leaves the project or is not a valid relative file path.".to_owned(),
        );
    }
    let relative: PathBuf = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if relative.as_os_str().is_empty() {
        return Err("Choose a file inside the project.".to_owned());
    }
    let target = root.join(relative);
    let mut ancestor = target.as_path();
    loop {
        match tokio::fs::symlink_metadata(ancestor).await {
            Ok(_) => {
                let canonical = tokio::fs::canonicalize(ancestor)
                    .await
                    .map_err(|error| format!("Cannot resolve that file or folder: {error}"))?;
                if !canonical.starts_with(root) {
                    return Err("That path resolves outside the project folder.".to_owned());
                }
                let suffix = target
                    .strip_prefix(ancestor)
                    .map_err(|error| error.to_string())?;
                return Ok(if suffix.as_os_str().is_empty() {
                    canonical
                } else {
                    canonical.join(suffix)
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| "The project folder is unavailable.".to_owned())?;
            }
            Err(error) => return Err(format!("Cannot access that file or folder: {error}")),
        }
    }
}

/// Bounded UTF-8 reads: a tool never sends a partial file while claiming it saw all of it.
pub async fn read(path: &Path) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("Cannot read the file: {error}"))?;
    let mut bytes = Vec::new();
    file.take(bhippi_types::WORKSPACE_TOOL_MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("Cannot read the file: {error}"))?;
    if bytes.len() as u64 > bhippi_types::WORKSPACE_TOOL_MAX_FILE_BYTES {
        return Err("The file is too large for this text tool; use a provider file tool that supports ranged reads.".to_owned());
    }
    if bytes.contains(&0) {
        return Err("This is a binary file; use the asset or application tools for it.".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| {
        "This file is not UTF-8 text; use the appropriate asset or application tool.".to_owned()
    })
}
