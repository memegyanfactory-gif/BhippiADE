//! The credits page a web publish ships beside `index.html` (GAD-092).
//!
//! INV-074 already stops an unlicensed asset from being exported at all, so by the time
//! this runs every asset under `assets/` has a `.meta.json` sidecar naming a real licence.
//! What is left is *saying so*: a page that names each asset, its licence and where it came
//! from, plus the engine's own credit, because Godot is MIT-licensed and a game that ships
//! its runtime owes that line.
//!
//! Nothing here reads the network, and nothing here decides whether a publish may proceed —
//! that is the gates' job. This module only renders what the gates already accepted.

use super::gates::LICENSE_SIDECAR_SUFFIX;
use super::ASSETS_DIR;
use std::path::{Path, PathBuf};

/// The file a publish writes into the export directory.
pub const CREDITS_FILE: &str = "credits.html";
/// The most assets one credits page lists. A pack with ten thousand tiles is a licence
/// statement, not a reading experience; past this the page says how many were elided.
pub const MAX_CREDITED_ASSETS: usize = 500;
/// How deep the sidecar walk goes under `assets/`.
const MAX_SCAN_DEPTH: usize = 12;

/// One credited asset, as the page lists it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetCredit {
    /// Project-relative, forward slashes.
    pub path: String,
    /// The sidecar's `license`, verbatim.
    pub license: String,
    /// The sidecar's `source` when it has one, else its `importer`.
    pub source: Option<String>,
}

/// Everything the export owes an attribution to, read from the sidecars themselves.
///
/// A sidecar that names no licence is *skipped rather than guessed at*: the release gates
/// have already refused the publish in that case, so reaching this function with one means
/// the caller ran in debug, and inventing "unknown" as an attribution would be worse than
/// leaving the asset off a page nobody is shipping.
#[must_use]
pub fn collect_credits(project_root: &Path) -> Vec<AssetCredit> {
    let mut sidecars = Vec::new();
    walk(
        &project_root.join(ASSETS_DIR),
        project_root,
        0,
        &mut sidecars,
    );
    sidecars.sort();

    let mut credits = Vec::new();
    for sidecar in sidecars {
        if credits.len() >= MAX_CREDITED_ASSETS {
            break;
        }
        let Ok(text) = std::fs::read_to_string(project_root.join(&sidecar)) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let license = value
            .get("license")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|license| {
                !license.is_empty() && !license.eq_ignore_ascii_case(super::gates::LICENSE_UNKNOWN)
            });
        let Some(license) = license else {
            continue;
        };
        let source = value
            .get("source")
            .or_else(|| value.get("importer"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(str::to_owned);
        credits.push(AssetCredit {
            path: sidecar
                .strip_suffix(LICENSE_SIDECAR_SUFFIX)
                .unwrap_or(&sidecar)
                .to_owned(),
            license: license.to_owned(),
            source,
        });
    }
    credits
}

/// The credits page itself.
///
/// Self-contained: one inline stylesheet, no fonts, no scripts, no network. A credits page
/// that needed a CDN would stop being a credits page the first time somebody opened the
/// export offline.
#[must_use]
pub fn render_credits_html(
    title: &str,
    description: &str,
    godot_version: &str,
    credits: &[AssetCredit],
) -> String {
    let mut rows = String::new();
    for credit in credits {
        rows.push_str(&format!(
            "      <tr><td>{path}</td><td>{license}</td><td>{source}</td></tr>\n",
            path = escape(&credit.path),
            license = escape(&credit.license),
            source = escape(credit.source.as_deref().unwrap_or("—")),
        ));
    }
    let assets_block = if credits.is_empty() {
        "    <p class=\"none\">This build ships no imported assets.</p>\n".to_owned()
    } else {
        format!(
            "    <table>\n      <caption>Assets in this build</caption>\n      \
             <thead><tr><th>File</th><th>Licence</th><th>Source</th></tr></thead>\n      \
             <tbody>\n{rows}      </tbody>\n    </table>\n"
        )
    };
    let elided = if credits.len() >= MAX_CREDITED_ASSETS {
        format!(
            "    <p class=\"note\">Only the first {MAX_CREDITED_ASSETS} assets are listed; the \
             project's own sidecars carry the rest.</p>\n"
        )
    } else {
        String::new()
    };
    let description_block = if description.trim().is_empty() {
        String::new()
    } else {
        format!("    <p class=\"lede\">{}</p>\n", escape(description.trim()))
    };

    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Credits — {title_escaped}</title>\n\
         <style>\n\
         :root {{ color-scheme: light dark; }}\n\
         body {{ margin: 0 auto; max-width: 46rem; padding: 2rem 1.25rem 4rem;\n\
           font: 15px/1.6 system-ui, -apple-system, Segoe UI, sans-serif; }}\n\
         h1 {{ font-size: 1.5rem; margin: 0 0 .25rem; }}\n\
         .lede {{ margin: 0 0 1.5rem; opacity: .8; }}\n\
         table {{ border-collapse: collapse; width: 100%; margin: 1rem 0 2rem; }}\n\
         caption {{ text-align: left; font-weight: 600; padding-bottom: .5rem; }}\n\
         th, td {{ text-align: left; padding: .4rem .6rem; border-bottom: 1px solid #8884; }}\n\
         td {{ font-variant-numeric: tabular-nums; word-break: break-word; }}\n\
         .note, .none {{ opacity: .75; font-size: .9rem; }}\n\
         footer {{ margin-top: 2rem; padding-top: 1rem; border-top: 1px solid #8884;\n\
           font-size: .9rem; opacity: .8; }}\n\
         </style>\n\
         </head>\n\
         <body>\n\
         <main>\n\
         \x20   <h1>{title_escaped}</h1>\n\
         {description_block}{assets_block}{elided}\
         \x20   <footer>\n\
         \x20     <p>Made with <a href=\"https://godotengine.org\">Godot Engine</a> \
         {godot_escaped} — MIT licence, © 2007–present Juan Linietsky, Ariel Manzur and \
         contributors.</p>\n\
         \x20     <p>Built with Bhippi.</p>\n\
         \x20   </footer>\n\
         </main>\n\
         </body>\n\
         </html>\n",
        title_escaped = escape(title),
        godot_escaped = escape(godot_version),
    )
}

/// Write `credits.html` into an export directory.
pub fn write_credits(project_root: &Path, export_dir: &Path) -> crate::error::Result<PathBuf> {
    let name = crate::manifest::load_manifest(project_root)?
        .map(|m| m.game.name)
        .unwrap_or_else(|| "Game".to_owned());
    let credits = collect_credits(project_root);
    let html = render_credits_html(&name, "", "4.7.1", &credits);
    let target = export_dir.join(CREDITS_FILE);
    std::fs::write(&target, html).map_err(|e| crate::error::EngineError::Io {
        operation: "write",
        path: target.display().to_string(),
        reason: e.to_string(),
        hint: None,
    })?;
    Ok(target)
}

/// The five characters that would otherwise let an asset path or a licence string close a
/// tag. An asset name is a file name from disk, so it is not trusted input.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Depth-limited walk collecting project-relative sidecar paths.
fn walk(directory: &Path, root: &Path, depth: usize, found: &mut Vec<String>) {
    if depth > MAX_SCAN_DEPTH || found.len() >= MAX_CREDITED_ASSETS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= MAX_CREDITED_ASSETS {
            return;
        }
        if path.is_dir() {
            walk(&path, root, depth + 1, found);
            continue;
        }
        let is_sidecar = path
            .file_name()
            .map(|name| name.to_string_lossy().ends_with(LICENSE_SIDECAR_SUFFIX))
            .unwrap_or(false);
        if !is_sidecar {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_credits, render_credits_html, AssetCredit};

    struct TempRoot(std::path::PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("bhippi-credits-{}", ulid::Ulid::new()));
            std::fs::create_dir_all(path.join("assets/models")).expect("temp root");
            Self(path)
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("parent");
            }
            std::fs::write(path, text).expect("write");
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ignored = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn credits_come_from_the_sidecars_and_an_unlicensed_asset_is_not_credited() {
        let root = TempRoot::new();
        root.write("assets/models/hero.glb", "binary");
        root.write(
            "assets/models/hero.glb.meta.json",
            r#"{"id":"01","license":"CC0-1.0","source":"Kenney"}"#,
        );
        root.write("assets/audio/jump.wav", "binary");
        root.write(
            "assets/audio/jump.wav.meta.json",
            r#"{"id":"02","license":"MIT","importer":"bhippi-procedural"}"#,
        );
        root.write("assets/models/mystery.glb", "binary");
        root.write(
            "assets/models/mystery.glb.meta.json",
            r#"{"id":"03","license":null}"#,
        );

        let credits = collect_credits(&root.0);
        assert_eq!(
            credits.len(),
            2,
            "the unlicensed asset is left off: {credits:?}"
        );
        assert_eq!(
            credits[0],
            AssetCredit {
                path: "assets/audio/jump.wav".to_owned(),
                license: "MIT".to_owned(),
                source: Some("bhippi-procedural".to_owned()),
            },
            "importer stands in for a missing source"
        );
        assert_eq!(credits[1].path, "assets/models/hero.glb");
        assert_eq!(credits[1].source.as_deref(), Some("Kenney"));
    }

    #[test]
    fn the_page_names_every_asset_the_engine_and_escapes_what_came_off_disk() {
        let html = render_credits_html(
            "Feather <Quest>",
            "Collect ten feathers.",
            "4.7.1",
            &[AssetCredit {
                path: "assets/models/a&b.glb".to_owned(),
                license: "CC-BY-4.0".to_owned(),
                source: None,
            }],
        );
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Feather &lt;Quest&gt;"));
        assert!(html.contains("assets/models/a&amp;b.glb"));
        assert!(html.contains("CC-BY-4.0"));
        assert!(html.contains("Collect ten feathers."));
        assert!(html.contains("Godot Engine</a> 4.7.1"));
        assert!(html.contains("MIT licence"));
        assert!(
            !html.contains("<Quest>"),
            "nothing raw off disk reaches the markup"
        );
        // Nothing external: a credits page has to work with the network unplugged.
        assert!(!html.contains("<script"));
        assert!(!html.contains("cdn"));
    }

    #[test]
    fn a_build_with_no_assets_says_so_rather_than_showing_an_empty_table() {
        let html = render_credits_html("Bare", "", "4.7.1", &[]);
        assert!(html.contains("ships no imported assets"));
        assert!(!html.contains("<table"));
        assert!(
            !html.contains("class=\"lede\""),
            "an empty description writes no paragraph"
        );
    }
}
