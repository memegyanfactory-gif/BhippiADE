//! Frozen, content-addressed games used to benchmark AI game generation.
//!
//! The corpus is intentionally provider-neutral and network-free. A benchmark case records
//! the exact prompt, seed, provider transcript, authored files and expected diagnostic codes.
//! Every artifact is content-addressed so changing an oracle is an explicit reviewed change.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub const GAME_QUALITY_CORPUS_SCHEMA: &str = "bhippi-game-quality-corpus@1";
pub const CANONICAL_GAME_COUNT: usize = 5;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameQualityCorpus {
    pub schema: String,
    pub cases: Vec<GameQualityCorpusCase>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameQualityCorpusCase {
    pub id: String,
    pub genre: String,
    pub seed: u64,
    pub prompt: FrozenCorpusArtifact,
    pub provider_transcript: FrozenCorpusArtifact,
    pub authored_files: Vec<FrozenCorpusArtifact>,
    pub expected_finding_codes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
pub struct FrozenCorpusArtifact {
    pub path: String,
    pub blake3: String,
}

impl GameQualityCorpus {
    pub fn parse(text: &str) -> Result<Self> {
        let corpus: Self = serde_json::from_str(text).map_err(|error| {
            corpus_error(
                &format!("invalid game quality corpus: {error}"),
                &format!("Fix the JSON and keep schema {GAME_QUALITY_CORPUS_SCHEMA}."),
            )
        })?;
        corpus.validate()?;
        Ok(corpus)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != GAME_QUALITY_CORPUS_SCHEMA {
            return Err(corpus_error(
                &format!("unsupported game quality corpus schema {:?}", self.schema),
                &format!("Use schema {GAME_QUALITY_CORPUS_SCHEMA}; unknown majors block."),
            ));
        }
        if self.cases.len() != CANONICAL_GAME_COUNT {
            return Err(corpus_error(
                &format!(
                    "the canonical quality corpus has {} cases, expected {CANONICAL_GAME_COUNT}",
                    self.cases.len()
                ),
                "Freeze all five benchmark games before changing the quality baseline.",
            ));
        }

        let mut case_ids = BTreeSet::new();
        let mut seeds = BTreeSet::new();
        let mut artifact_paths = BTreeSet::new();
        for case in &self.cases {
            require_token(&case.id, "case id")?;
            require_token(&case.genre, "case genre")?;
            if !case_ids.insert(case.id.as_str()) {
                return Err(corpus_error(
                    &format!("duplicate quality corpus case {:?}", case.id),
                    "Give every benchmark game a unique stable id.",
                ));
            }
            if !seeds.insert(case.seed) {
                return Err(corpus_error(
                    &format!("duplicate quality corpus seed {}", case.seed),
                    "Pin an independent seed for every benchmark game.",
                ));
            }
            if case.authored_files.is_empty() {
                return Err(corpus_error(
                    &format!("quality corpus case {:?} has no authored files", case.id),
                    "Freeze the generated manifest and authored content for this case.",
                ));
            }

            let prefix = format!("corpus-v1/{}/", case.id);
            for artifact in std::iter::once(&case.prompt)
                .chain(std::iter::once(&case.provider_transcript))
                .chain(case.authored_files.iter())
            {
                validate_artifact(artifact, &prefix)?;
                if !artifact_paths.insert(artifact.path.as_str()) {
                    return Err(corpus_error(
                        &format!("duplicate quality corpus artifact {:?}", artifact.path),
                        "Reference each frozen artifact exactly once.",
                    ));
                }
            }

            let mut finding_codes = BTreeSet::new();
            for code in &case.expected_finding_codes {
                let valid = code.len() == 10
                    && code.starts_with("BHP-GD-")
                    && code[7..].bytes().all(|byte| byte.is_ascii_digit());
                if !valid || !finding_codes.insert(code.as_str()) {
                    return Err(corpus_error(
                        &format!("invalid or duplicate expected finding code {code:?}"),
                        "Use each stable BHP-GD-NNN diagnostic code at most once.",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Verify all frozen files under the directory containing the corpus manifest.
    pub fn verify_at(&self, fixture_root: &Path) -> Result<()> {
        self.validate()?;
        for case in &self.cases {
            verify_artifact(fixture_root, &case.prompt, false)?;
            verify_artifact(fixture_root, &case.provider_transcript, true)?;
            for artifact in &case.authored_files {
                verify_artifact(fixture_root, artifact, false)?;
            }
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| {
            corpus_error(
                &format!("cannot serialise game quality corpus: {error}"),
                "Report this as an engine bug.",
            )
        })
    }
}

fn validate_artifact(artifact: &FrozenCorpusArtifact, required_prefix: &str) -> Result<()> {
    if !artifact.path.starts_with(required_prefix)
        || artifact.path.contains('\\')
        || !Path::new(&artifact.path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(corpus_error(
            &format!(
                "unsafe or cross-case corpus artifact path {:?}",
                artifact.path
            ),
            "Use a forward-slash relative path inside this case's corpus-v1 directory.",
        ));
    }
    if artifact.blake3.len() != 64
        || !artifact
            .blake3
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(corpus_error(
            &format!("artifact {:?} has an invalid BLAKE3 digest", artifact.path),
            "Freeze the exact file bytes as a 64-character lowercase BLAKE3 digest.",
        ));
    }
    Ok(())
}

fn verify_artifact(root: &Path, artifact: &FrozenCorpusArtifact, json: bool) -> Result<()> {
    let path = root.join(&artifact.path);
    let bytes = std::fs::read(&path).map_err(|error| {
        corpus_error(
            &format!(
                "cannot read frozen corpus artifact {:?}: {error}",
                artifact.path
            ),
            "Restore the committed fixture or explicitly update the corpus manifest.",
        )
    })?;
    if bytes.is_empty() {
        return Err(corpus_error(
            &format!("frozen corpus artifact {:?} is empty", artifact.path),
            "Commit the exact benchmark input or authored output.",
        ));
    }
    if json {
        serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| {
            corpus_error(
                &format!(
                    "provider transcript {:?} is invalid JSON: {error}",
                    artifact.path
                ),
                "Freeze the provider exchange as parseable JSON.",
            )
        })?;
    }
    let actual = blake3::hash(&bytes).to_hex().to_string();
    if actual != artifact.blake3 {
        return Err(corpus_error(
            &format!(
                "frozen corpus artifact {:?} changed: expected {}, found {actual}",
                artifact.path, artifact.blake3
            ),
            "Revert the accidental drift, or review and update the benchmark oracle together.",
        ));
    }
    Ok(())
}

fn require_token(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 80 {
        return Err(corpus_error(
            &format!("quality corpus {label} is empty or too long"),
            "Use a stable non-empty token no longer than 80 characters.",
        ));
    }
    Ok(())
}

fn corpus_error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
