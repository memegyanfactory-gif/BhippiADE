//! Sandboxed skill registry, discovery, and execution.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A recognized AI skill imported from a pre-installed AI app or defined by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    /// Source provider / app: "claude", "codex", "antigravity", "cursor", "workspace", "builtin", "custom"
    pub source: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub path: Option<String>,
}

/// Skill configuration and override states persisted to disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillOverrides {
    pub enabled_map: HashMap<String, bool>,
    pub custom_skills: Vec<Skill>,
}

/// Skill store that handles discovery across pre-installed AI apps and local workspace.
#[derive(Debug, Clone)]
pub struct SkillStore {
    state_file: PathBuf,
}

impl SkillStore {
    pub fn new(state_file: PathBuf) -> Self {
        Self { state_file }
    }

    pub fn default_path() -> Option<PathBuf> {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .ok()?;
        Some(PathBuf::from(home).join(".bhippi").join("skills.json"))
    }

    /// Loads overrides from `~/.bhippi/skills.json`.
    pub async fn load_overrides(&self) -> SkillOverrides {
        match tokio::fs::read_to_string(&self.state_file).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => SkillOverrides::default(),
        }
    }

    /// Saves overrides to `~/.bhippi/skills.json`.
    pub async fn save_overrides(&self, overrides: &SkillOverrides) -> Result<(), String> {
        if let Some(parent) = self.state_file.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let json = serde_json::to_string_pretty(overrides)
            .map_err(|err| format!("Failed to serialize skills: {err}"))?;
        tokio::fs::write(&self.state_file, json)
            .await
            .map_err(|err| format!("Failed to write skills state: {err}"))?;
        Ok(())
    }

    /// Sets the enabled state for a specific skill.
    pub async fn set_skill_enabled(&self, skill_id: &str, enabled: bool) -> Result<(), String> {
        let mut overrides = self.load_overrides().await;
        overrides.enabled_map.insert(skill_id.to_owned(), enabled);
        self.save_overrides(&overrides).await
    }

    /// Discovers all skills from Claude, Codex, Antigravity/Gemini, Cursor, workspace, and builtins.
    pub async fn list_skills(&self, workspace: Option<&Path>) -> Vec<Skill> {
        let overrides = self.load_overrides().await;
        let mut discovered = discover_external_skills(workspace).await;

        // Add custom skills from overrides
        for custom in &overrides.custom_skills {
            if !discovered.iter().any(|s| s.id == custom.id) {
                discovered.push(custom.clone());
            }
        }

        // Apply enabled overrides
        for skill in &mut discovered {
            if let Some(&enabled) = overrides.enabled_map.get(&skill.id) {
                skill.enabled = enabled;
            }
        }

        // Sort by source then name
        discovered.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));

        discovered
    }
}

/// Discovers skills from all pre-installed AI apps and directories.
pub async fn discover_external_skills(workspace: Option<&Path>) -> Vec<Skill> {
    let mut skills = Vec::new();
    let mut seen_ids = HashSet::new();

    let home_opt = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from);

    // 1. Google Antigravity / Gemini CLI skills
    if let Some(ref home) = home_opt {
        let gemini_config_skills = home.join(".gemini").join("config").join("skills");
        scan_skills_directory(
            &gemini_config_skills,
            "antigravity",
            &mut skills,
            &mut seen_ids,
        )
        .await;

        let gemini_plugins = home.join(".gemini").join("config").join("plugins");
        scan_plugins_directory(&gemini_plugins, "antigravity", &mut skills, &mut seen_ids).await;

        let antigravity_builtin = home
            .join(".gemini")
            .join("antigravity-ide")
            .join("builtin")
            .join("skills");
        scan_skills_directory(
            &antigravity_builtin,
            "antigravity",
            &mut skills,
            &mut seen_ids,
        )
        .await;
    }

    // 2. Claude Code skills & commands (~/.claude/commands/, ~/.claude/skills/, ~/.anthropic/)
    if let Some(ref home) = home_opt {
        let claude_skills = home.join(".claude").join("skills");
        scan_skills_directory(&claude_skills, "claude", &mut skills, &mut seen_ids).await;

        let claude_commands = home.join(".claude").join("commands");
        scan_prompt_files(&claude_commands, "claude", &mut skills, &mut seen_ids).await;

        let anthropic_skills = home.join(".anthropic").join("skills");
        scan_skills_directory(&anthropic_skills, "claude", &mut skills, &mut seen_ids).await;
    }

    // 3. OpenAI Codex skills (~/.codex/skills/, ~/.codex/prompts/)
    if let Some(ref home) = home_opt {
        let codex_skills = home.join(".codex").join("skills");
        scan_skills_directory(&codex_skills, "codex", &mut skills, &mut seen_ids).await;

        let codex_prompts = home.join(".codex").join("prompts");
        scan_prompt_files(&codex_prompts, "codex", &mut skills, &mut seen_ids).await;

        let bionic_skills = home.join(".bionic").join("skills");
        scan_skills_directory(&bionic_skills, "bionic", &mut skills, &mut seen_ids).await;

        let bionic_prompts = home.join(".bionic").join("prompts");
        scan_prompt_files(&bionic_prompts, "bionic", &mut skills, &mut seen_ids).await;
    }

    // 4. Workspace skills (.agents/skills/, .bhippi/skills/, .cursorrules)
    if let Some(ws) = workspace {
        let ws_agents_skills = ws.join(".agents").join("skills");
        scan_skills_directory(&ws_agents_skills, "workspace", &mut skills, &mut seen_ids).await;

        let ws_bhippi_skills = ws.join(".bhippi").join("skills");
        scan_skills_directory(&ws_bhippi_skills, "workspace", &mut skills, &mut seen_ids).await;

        let cursor_rules = ws.join(".cursorrules");
        if cursor_rules.is_file() {
            if let Ok(content) = tokio::fs::read_to_string(&cursor_rules).await {
                if !seen_ids.contains("cursor-project-rules") {
                    seen_ids.insert("cursor-project-rules".to_owned());
                    skills.push(Skill {
                        id: "cursor-project-rules".to_owned(),
                        name: "Cursor Project Rules".to_owned(),
                        description:
                            "Rules and instructions imported from .cursorrules in workspace."
                                .to_owned(),
                        prompt: content,
                        source: "cursor".to_owned(),
                        tags: vec![
                            "cursor".to_owned(),
                            "rules".to_owned(),
                            "workspace".to_owned(),
                        ],
                        enabled: true,
                        path: Some(cursor_rules.to_string_lossy().to_string()),
                    });
                }
            }
        }
    }

    // 5. Always supply built-in engineering core skills
    for builtin in builtin_skills() {
        if !seen_ids.contains(&builtin.id) {
            seen_ids.insert(builtin.id.clone());
            skills.push(builtin);
        }
    }

    skills
}

/// Scans a directory containing skill subdirectories, each having a `SKILL.md` file.
async fn scan_skills_directory(
    dir: &Path,
    source: &str,
    out: &mut Vec<Skill>,
    seen: &mut HashSet<String>,
) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                if let Ok(content) = tokio::fs::read_to_string(&skill_file).await {
                    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("skill");
                    let id = format!("{source}-{dir_name}");
                    if !seen.contains(&id) {
                        seen.insert(id.clone());
                        let (name, description, prompt) = parse_skill_markdown(dir_name, &content);
                        out.push(Skill {
                            id,
                            name,
                            description,
                            prompt,
                            source: source.to_owned(),
                            tags: vec![source.to_owned(), "skill".to_owned()],
                            enabled: true,
                            path: Some(skill_file.to_string_lossy().to_string()),
                        });
                    }
                }
            }
        }
    }
}

/// Scans plugins directory where each plugin folder has `skills/` subdirectory.
async fn scan_plugins_directory(
    dir: &Path,
    source: &str,
    out: &mut Vec<Skill>,
    seen: &mut HashSet<String>,
) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_dir() {
            let plugin_skills = path.join("skills");
            if plugin_skills.is_dir() {
                scan_skills_directory(&plugin_skills, source, out, seen).await;
            }
        }
    }
}

/// Scans a directory of prompt markdown or text files (`.md`, `.prompt`, `.txt`).
async fn scan_prompt_files(
    dir: &Path,
    source: &str,
    out: &mut Vec<Skill>,
    seen: &mut HashSet<String>,
) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "md" || ext == "prompt" || ext == "txt" {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    let stem = path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or("prompt");
                    let id = format!("{source}-{stem}");
                    if !seen.contains(&id) {
                        seen.insert(id.clone());
                        let (name, description, prompt) = parse_skill_markdown(stem, &content);
                        out.push(Skill {
                            id,
                            name,
                            description,
                            prompt,
                            source: source.to_owned(),
                            tags: vec![source.to_owned(), "command".to_owned()],
                            enabled: true,
                            path: Some(path.to_string_lossy().to_string()),
                        });
                    }
                }
            }
        }
    }
}

/// Parses YAML frontmatter or Markdown heading from SKILL.md.
fn parse_skill_markdown(fallback_id: &str, content: &str) -> (String, String, String) {
    let trimmed = content.trim();

    // Check for YAML frontmatter between `---`
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end_idx) = rest.find("---") {
            let frontmatter = &rest[..end_idx];
            let body = rest[end_idx + 3..].trim();

            let mut name = None;
            let mut description = None;

            for line in frontmatter.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("name:") {
                    name = Some(val.trim().trim_matches('"').trim_matches('\'').to_owned());
                } else if let Some(val) = line.strip_prefix("description:") {
                    description = Some(val.trim().trim_matches('"').trim_matches('\'').to_owned());
                }
            }

            let final_name = name.unwrap_or_else(|| humanize_id(fallback_id));
            let final_desc = description.unwrap_or_else(|| format!("Skill for {final_name}"));
            return (final_name, final_desc, body.to_owned());
        }
    }

    // Fallback: search for first `# Title`
    let mut lines = content.lines();
    let mut title = None;
    let mut desc = None;

    for line in lines.by_ref() {
        let trimmed_line = line.trim();
        if let Some(t) = trimmed_line.strip_prefix("# ") {
            title = Some(t.trim().to_owned());
            break;
        }
    }

    for line in lines.by_ref() {
        let trimmed_line = line.trim();
        if !trimmed_line.is_empty() && !trimmed_line.starts_with('#') {
            desc = Some(trimmed_line.to_owned());
            break;
        }
    }

    let final_name = title.unwrap_or_else(|| humanize_id(fallback_id));
    let final_desc = desc.unwrap_or_else(|| format!("Skill for {final_name}"));
    (final_name, final_desc, content.to_owned())
}

fn humanize_id(id: &str) -> String {
    id.replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fallback built-in engineering skills available out-of-the-box.
fn builtin_skills() -> Vec<Skill> {
    vec![
        Skill {
            id: "builtin-code-optimizer".to_owned(),
            name: "Code Optimizer & Performance".to_owned(),
            description: "Analyzes algorithmic complexity, memory allocations, and performance bottlenecks.".to_owned(),
            prompt: "When writing or reviewing code, analyze performance, minimize unneeded allocations, suggest zero-copy idioms, and explain time/space trade-offs.".to_owned(),
            source: "builtin".to_owned(),
            tags: vec!["performance".to_owned(), "optimization".to_owned()],
            enabled: true,
            path: None,
        },
        Skill {
            id: "builtin-test-craftsman".to_owned(),
            name: "Test Generation & TDD".to_owned(),
            description: "Generates comprehensive unit, property, and edge-case tests with regression assertions.".to_owned(),
            prompt: "For any code change, write idiomatic, high-coverage unit tests including edge cases, failure branches, and regression tests.".to_owned(),
            source: "builtin".to_owned(),
            tags: vec!["testing".to_owned(), "qa".to_owned()],
            enabled: true,
            path: None,
        },
        Skill {
            id: "builtin-a11y-auditor".to_owned(),
            name: "Accessibility (a11y) Auditor".to_owned(),
            description: "Audits semantic HTML, ARIA labels, focus states, keyboard navigation, and contrast.".to_owned(),
            prompt: "Enforce WCAG 2.1 AA accessibility standards: ensure semantic HTML elements, accessible names via aria-label/aria-labelledby, focus rings, and keyboard tabability.".to_owned(),
            source: "builtin".to_owned(),
            tags: vec!["accessibility".to_owned(), "frontend".to_owned()],
            enabled: true,
            path: None,
        },
        Skill {
            id: "builtin-security-auditor".to_owned(),
            name: "Security & Invariant Guard".to_owned(),
            description: "Detects path traversal, injection vectors, unauthenticated endpoints, and invariant breaks.".to_owned(),
            prompt: "Audit code for security vulnerabilities: path traversal, command injection, secret leakage, unvalidated inputs, and permission boundary violations.".to_owned(),
            source: "builtin".to_owned(),
            tags: vec!["security".to_owned(), "invariants".to_owned()],
            enabled: true,
            path: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_yaml_frontmatter_skill() {
        let content = r#"---
name: "Modern Web Guidance"
description: "Best practices for HTML5 and CSS."
---

# Instructions
Always use semantic tags."#;

        let (name, desc, body) = parse_skill_markdown("modern-web", content);
        assert_eq!(name, "Modern Web Guidance");
        assert_eq!(desc, "Best practices for HTML5 and CSS.");
        assert!(body.contains("Always use semantic tags"));
    }

    #[test]
    fn parse_markdown_fallback_skill() {
        let content = r#"# Architecture Reviewer
Review crate boundaries and circular dependencies.

Detailed guidelines follow here."#;

        let (name, desc, body) = parse_skill_markdown("arch-rev", content);
        assert_eq!(name, "Architecture Reviewer");
        assert_eq!(desc, "Review crate boundaries and circular dependencies.");
        assert_eq!(body, content);
    }

    #[test]
    fn builtin_skills_have_unique_ids() {
        let builtins = builtin_skills();
        let mut ids = HashSet::new();
        for b in builtins {
            assert!(ids.insert(b.id), "Duplicate builtin skill id");
        }
    }
}
