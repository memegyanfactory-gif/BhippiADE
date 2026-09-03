//! Generates the Phase 8 10-game corpus v2 fixtures and quality baseline.
//!
//! Run: `cargo run -p bhippi-engine --bin generate-corpus-v2`

use bhippi_engine::game_quality_baseline::{
    evaluate_static_corpus, GameQualityBaseline, QualityRegressionPolicy,
};
use bhippi_engine::game_quality_corpus::{
    FrozenCorpusArtifact, GameQualityCorpus, GameQualityCorpusCase, GAME_QUALITY_CORPUS_SCHEMA_V2,
};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use bhippi_engine::intent::delta::spec_from_draft;
use bhippi_engine::intent::draft::draft;
use std::path::{Path, PathBuf};

struct CaseDef {
    id: &'static str,
    genre: &'static str,
    seed: u64,
    prompt: &'static str,
    template: ProjectTemplate,
}

const CASES: [CaseDef; 10] = [
    CaseDef {
        id: "runner-horizon",
        genre: "endless_runner",
        seed: 20001,
        prompt: "an endless runner dodging trains and collecting coins on three tracks",
        template: ProjectTemplate::ThirdPerson3D,
    },
    CaseDef {
        id: "exploration-island",
        genre: "exploration",
        seed: 20002,
        prompt: "a cozy third-person exploration game with jump-and-glide, low-poly islands, collect 10 feathers to unlock the lighthouse",
        template: ProjectTemplate::ThirdPerson3D,
    },
    CaseDef {
        id: "arena-shooter",
        genre: "fps_arena",
        seed: 20003,
        prompt: "a fast sci-fi arena first person shooter with laser blasters, frag limit",
        template: ProjectTemplate::Empty3D,
    },
    CaseDef {
        id: "platformer-retro",
        genre: "platformer_2d",
        seed: 20004,
        prompt: "a 2d retro platformer with wall jumps, collect coins, avoid spikes",
        template: ProjectTemplate::TopDown2D,
    },
    CaseDef {
        id: "platformer-3d",
        genre: "platformer_3d",
        seed: 20005,
        prompt: "a 3d platformer where you double jump across floating islands to collect stars",
        template: ProjectTemplate::ThirdPerson3D,
    },
    CaseDef {
        id: "puzzle-stacker",
        genre: "puzzle_physics",
        seed: 20006,
        prompt: "a physics puzzle game where you balance and stack blocks to reach the goal",
        template: ProjectTemplate::Empty3D,
    },
    CaseDef {
        id: "kart-circuit",
        genre: "racing_kart",
        seed: 20007,
        prompt: "an arcade kart racing game with three laps, drift boosts and powerups",
        template: ProjectTemplate::ThirdPerson3D,
    },
    CaseDef {
        id: "forest-survival",
        genre: "survival",
        seed: 20008,
        prompt: "a wilderness survival game gathering wood and building a shelter before night",
        template: ProjectTemplate::ThirdPerson3D,
    },
    CaseDef {
        id: "dungeon-slasher",
        genre: "top_down_action",
        seed: 20009,
        prompt: "a top down action dungeon crawler fighting skeleton waves",
        template: ProjectTemplate::TopDown2D,
    },
    CaseDef {
        id: "tower-defense",
        genre: "tower_defense",
        seed: 20010,
        prompt: "a tower defense game placing turrets along a path to defend the core",
        template: ProjectTemplate::Empty3D,
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = manifest_dir.join("../../tests/fixtures/engine/quality");
    let corpus_v2_root = fixture_root.join("corpus-v2");

    println!("fixture_root: {}", fixture_root.display());
    std::fs::create_dir_all(&corpus_v2_root)?;

    let mut corpus_cases = Vec::new();

    for def in &CASES {
        let case_dir = corpus_v2_root.join(def.id);
        let authored_dir = case_dir.join("authored");
        std::fs::create_dir_all(&authored_dir)?;

        // 1. Write prompt.txt
        let prompt_path = case_dir.join("prompt.txt");
        std::fs::write(&prompt_path, def.prompt)?;
        let prompt_artifact = make_artifact(&fixture_root, &prompt_path)?;

        // 2. Write provider-transcript.json
        let transcript_json = serde_json::json!({
            "schema": "bhippi-transcript@1",
            "provider": "demo",
            "model": "demo-v1",
            "messages": [
                {
                    "role": "user",
                    "content": def.prompt
                },
                {
                    "role": "assistant",
                    "content": format!("Plan established for {}. Generated starter scene.", def.genre)
                }
            ]
        });
        let transcript_path = case_dir.join("provider-transcript.json");
        std::fs::write(
            &transcript_path,
            serde_json::to_string_pretty(&transcript_json)?,
        )?;
        let transcript_artifact = make_artifact(&fixture_root, &transcript_path)?;

        // 3. Scaffold Godot project into authored_dir
        write_project(&authored_dir, def.id, def.template, true)?;

        // 4. Draft intent and generate game_spec.json
        let intent_draft = draft(def.prompt);
        let pack = bhippi_engine::intent::archetype::builtin()
            .into_iter()
            .find(|p| p.id == def.genre)
            .ok_or_else(|| format!("unknown archetype {}", def.genre))?;
        let spec = spec_from_draft(&intent_draft, &pack);
        let spec_path = authored_dir.join("game_spec.json");
        std::fs::write(&spec_path, serde_json::to_string_pretty(&spec)?)?;

        // 5. Collect authored files
        let script_rel = def.template.script_rel();
        let authored_files = vec![
            make_artifact(&fixture_root, &authored_dir.join("Bhippi.game.toml"))?,
            make_artifact(&fixture_root, &authored_dir.join("game_spec.json"))?,
            make_artifact(&fixture_root, &authored_dir.join("project.godot"))?,
            make_artifact(&fixture_root, &authored_dir.join("scenes/main.tscn"))?,
            make_artifact(&fixture_root, &authored_dir.join(script_rel))?,
        ];

        corpus_cases.push(GameQualityCorpusCase {
            id: def.id.to_owned(),
            genre: def.genre.to_owned(),
            seed: def.seed,
            prompt: prompt_artifact,
            provider_transcript: transcript_artifact,
            authored_files,
            expected_finding_codes: Vec::new(),
        });
    }

    let corpus = GameQualityCorpus {
        schema: GAME_QUALITY_CORPUS_SCHEMA_V2.to_owned(),
        cases: corpus_cases,
    };
    corpus.validate()?;
    corpus.verify_at(&fixture_root)?;

    let corpus_json_path = fixture_root.join("quality-corpus-v2.json");
    std::fs::write(&corpus_json_path, corpus.dump()?)?;
    println!("wrote {}", corpus_json_path.display());

    // 6. Evaluate static corpus and create quality-baseline-v2.json
    let run = evaluate_static_corpus(&corpus, &fixture_root)?;
    let baseline = GameQualityBaseline::record(&corpus, &run, QualityRegressionPolicy::default())?;
    let baseline_json_path = fixture_root.join("quality-baseline-v2.json");
    std::fs::write(&baseline_json_path, baseline.dump()?)?;
    println!("wrote {}", baseline_json_path.display());

    // 7. Write quality-rubric-v2.json
    let rubric_json = serde_json::json!({
        "schema": "bhippi-game-quality-rubric@2",
        "name": "Godot 4 ADE Quality Rubric v2",
        "description": "Evidence-backed 10-dimension quality rubric for Godot 4 games generated by Bhippi Studio",
        "dimensions": [
            { "id": "bootability", "description": "Game boots without crashing or fatal Godot errors" },
            { "id": "goal_clarity", "description": "Win and lose conditions are clear and reachable" },
            { "id": "control_correctness", "description": "Input mapping responds smoothly and predictably" },
            { "id": "progression_finishability", "description": "Levels, objectives, or laps can be completed" },
            { "id": "failure_recovery", "description": "Player respawns or restarts cleanly on defeat" },
            { "id": "runtime_stability", "description": "Zero unhandled exceptions or orphan nodes during play" },
            { "id": "visual_legibility", "description": "Camera framing, lighting, and materials are clear" },
            { "id": "hud_feedback", "description": "Score, health, or objective progress displays on screen" },
            { "id": "content_coherence", "description": "Visual style, assets, and theme match the archetype" },
            { "id": "performance", "description": "Sustained frame rate >= 60 FPS without memory leaks" }
        ],
        "invariant": "Unobserved dimensions stay not_measured and never become zero."
    });
    let rubric_path = fixture_root.join("quality-rubric-v2.json");
    std::fs::write(&rubric_path, serde_json::to_string_pretty(&rubric_json)?)?;
    println!("wrote {}", rubric_path.display());

    Ok(())
}

fn make_artifact(
    fixture_root: &Path,
    file_path: &Path,
) -> Result<FrozenCorpusArtifact, Box<dyn std::error::Error>> {
    let rel = file_path
        .strip_prefix(fixture_root)
        .map_err(|e| {
            format!(
                "cannot strip prefix {} from {}: {e}",
                fixture_root.display(),
                file_path.display()
            )
        })?
        .to_string_lossy()
        .replace('\\', "/");
    let bytes = std::fs::read(file_path)
        .map_err(|e| format!("cannot read {}: {e}", file_path.display()))?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    Ok(FrozenCorpusArtifact {
        path: rel,
        blake3: hash,
    })
}
