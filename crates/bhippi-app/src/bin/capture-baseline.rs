//! Captures the Token Engine Phase-A baseline.
//!
//! Rust by ANY other bin in this crate would be a separate crate root, so the real
//! work lives in `bhippi_app::token_baseline` (inside the lib, where the
//! pub(crate) turn machinery is visible); this bin is a thin async entry point.
//!
//! Writes `docs/token-engine/baseline.json` + `baseline.md`.
//!
//! `--engine` instead captures the GAD-040 engine-turn baseline (the ENG-418 task set run
//! inside a real Godot project fixture), writing `docs/token-engine/engine-baseline.json` +
//! `engine-baseline.md`. The two modes are independent; passing neither/the default runs the
//! original plain-chat baseline exactly as before.

use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("token-engine");

    if std::env::args().any(|arg| arg == "--engine") {
        capture_engine(&output_dir).await;
        return;
    }

    match bhippi_app::token_baseline::capture_into(&output_dir).await {
        Ok(report) => {
            println!("baseline captured");
            println!("  samples : {}", report.samples_json.display());
            println!("  report  : {}", report.report_md.display());
            println!(
                "  tasks   : {} ({} multi-turn follow-ups)",
                report.tasks.len(),
                report
                    .tasks
                    .iter()
                    .filter(|task| task.history_messages > 1)
                    .count()
            );
        }
        Err(error) => {
            eprintln!("capture-baseline failed: {error}");
            std::process::exit(1);
        }
    }
}

async fn capture_engine(output_dir: &std::path::Path) {
    match bhippi_app::token_baseline::capture_engine_into(output_dir).await {
        Ok(report) => {
            println!("engine baseline captured");
            println!("  report (json) : {}", report.report_json.display());
            println!("  report (md)   : {}", report.report_md.display());
            println!(
                "  runs          : {} ({} distinct tasks, {} multi-turn follow-ups)",
                report.runs.len(),
                report
                    .runs
                    .iter()
                    .map(|run| run.label.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                report
                    .runs
                    .iter()
                    .filter(|run| run.history_messages > 1)
                    .count()
            );
            println!(
                "  doctrine      : {} tokens ({} bytes of prompts/chat-engine.md)",
                report.chat_engine_tokens, report.chat_engine_bytes
            );
        }
        Err(error) => {
            eprintln!("capture-baseline --engine failed: {error}");
            std::process::exit(1);
        }
    }
}
