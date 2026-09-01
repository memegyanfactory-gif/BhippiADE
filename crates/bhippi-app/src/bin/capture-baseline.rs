//! Captures the Token Engine Phase-A baseline.
//!
//! Rust by ANY other bin in this crate would be a separate crate root, so the real
//! work lives in `bhippi_app::token_baseline` (inside the lib, where the
//! pub(crate) turn machinery is visible); this bin is a thin async entry point.
//!
//! Writes `docs/token-engine/baseline.json` + `baseline.md`.

use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("token-engine");

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
