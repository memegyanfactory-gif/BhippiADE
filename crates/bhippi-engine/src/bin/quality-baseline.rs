//! Offline command for recording and checking the deterministic game-quality baseline.

use bhippi_engine::game_quality_baseline::{
    compare_quality_run, evaluate_static_corpus, GameQualityBaseline, GameQualityRun,
    QualityRegressionPolicy,
};
use bhippi_engine::game_quality_corpus::GameQualityCorpus;
use std::io::Write as _;
use std::path::Path;

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("quality-baseline failed: {error}");
        std::process::exit(1);
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    match arguments.as_slice() {
        [command, corpus_path, fixture_root, output_path] if command == "evaluate-static" => {
            let corpus = read_corpus(Path::new(corpus_path))?;
            let run = evaluate_static_corpus(&corpus, Path::new(fixture_root))
                .map_err(|error| error.to_string())?;
            write_new(Path::new(output_path), &run.dump().map_err(|error| error.to_string())?)?;
            println!("quality run written to {output_path}");
            Ok(())
        }
        [command, corpus_path, run_path, output_path] if command == "record" => {
            let corpus = read_corpus(Path::new(corpus_path))?;
            let run = GameQualityRun::parse(&read(Path::new(run_path))?)
                .map_err(|error| error.to_string())?;
            let baseline = GameQualityBaseline::record(
                &corpus,
                &run,
                QualityRegressionPolicy::default(),
            )
            .map_err(|error| error.to_string())?;
            write_new(
                Path::new(output_path),
                &baseline.dump().map_err(|error| error.to_string())?,
            )?;
            println!("quality baseline written to {output_path}");
            Ok(())
        }
        [command, corpus_path, baseline_path, run_path, output_path] if command == "check" => {
            let corpus = read_corpus(Path::new(corpus_path))?;
            let baseline = GameQualityBaseline::parse(&read(Path::new(baseline_path))?)
                .map_err(|error| error.to_string())?;
            let run = GameQualityRun::parse(&read(Path::new(run_path))?)
                .map_err(|error| error.to_string())?;
            let comparison = compare_quality_run(&corpus, &baseline, &run)
                .map_err(|error| error.to_string())?;
            write_new(
                Path::new(output_path),
                &comparison.dump().map_err(|error| error.to_string())?,
            )?;
            println!(
                "quality comparison written to {output_path}: {}",
                if comparison.passed { "passed" } else { "failed" }
            );
            if comparison.passed {
                Ok(())
            } else {
                Err("candidate quality evidence regressed from the committed baseline".to_owned())
            }
        }
        _ => Err(
            "usage:\n  quality-baseline evaluate-static <corpus.json> <fixture-root> <run.json>\n  quality-baseline record <corpus.json> <run.json> <baseline.json>\n  quality-baseline check <corpus.json> <baseline.json> <run.json> <comparison.json>"
                .to_owned(),
        ),
    }
}

fn read_corpus(path: &Path) -> Result<GameQualityCorpus, String> {
    GameQualityCorpus::parse(&read(path)?).map_err(|error| error.to_string())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// Baseline commands never silently replace reviewable evidence. Callers write to a new path,
/// inspect the diff and explicitly remove the old artifact when accepting a new baseline.
fn write_new(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    if let Err(error) = file
        .write_all(text.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
    {
        let _ignored = std::fs::remove_file(path);
        return Err(format!("cannot write {}: {error}", path.display()));
    }
    Ok(())
}
