//! The Computer Use playtest loop against a real Godot window (ADR-0044, GAD-095…099).
//!
//! `godot_live_app.rs` proves the headless half: the flags, the scaffold, the telemetry file.
//! This one proves the half that needs a screen. It scaffolds a ThirdPerson3D project, runs
//! the default **Watch play** plan against it, and asserts what the evidence pair has to
//! contain for a build run to be allowed to read it:
//!
//! 1. at least two frames, each a real PNG with a recorded capture method;
//! 2. the window it photographed is the game's — class `Engine`, titled with the game name,
//!    and a different handle from anything that was on the desktop beforehand;
//! 3. telemetry with its `done` line and at least five samples, which together mean the game
//!    shut down cleanly rather than being killed mid-frame;
//! 4. the player moved between the first and last *paired* sample — proof the keystrokes
//!    reached the game rather than the desktop.
//!
//! `#[ignore]`, and for a stronger reason than the other live tests: **a real Godot window
//! opens on this desktop for a few seconds and Bhippi types into it.** Run it deliberately:
//!
//! ```text
//! set BHIPPI_GODOT=C:\...\Godot_v4.7.1-stable_win64_console.exe
//! cargo test -p bhippi-app --test godot_visual_live -- --ignored --nocapture
//! ```
//!
//! `BHIPPI_GODOT` points at the **console** build, as everywhere else; the GUI binary the
//! window comes from is derived from it by `pair_windows_binaries`, because a windowed run
//! through the console build flashes a console behind the game.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_app::computer_window::find_windows;
use bhippi_app::godot::{detect_godot, stop_channel};
use bhippi_app::godot_observe::{
    game_window_filter, requires_vision, run_visual_playtest, VisualLaunch, VisualPlaytestPlan,
    VisualStopReason,
};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The game's name, which is also what Godot puts in the window title.
const GAME_NAME: &str = "Watch Play Live";
/// The fewest frames a run that opened a window has to bring back.
const MIN_CAPTURES: usize = 2;
/// The fewest telemetry samples the probe has to have written.
const MIN_SAMPLES: usize = 5;
/// How far the player has to have moved for the keystrokes to have landed.
const MIN_TRAVEL: f64 = 0.5;

fn workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi-godot-visual-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp workspace");
    dir
}

/// Straight-line distance between two sampled positions, in whichever dimension count the
/// sample carries.
fn travelled(first: &[f64], last: &[f64]) -> f64 {
    first
        .iter()
        .zip(last.iter())
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f64>()
        .sqrt()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens a real Godot window on this desktop and types into it; set BHIPPI_GODOT"]
async fn bhippi_watches_and_plays_a_real_game_window() {
    let Some(install) = detect_godot(None).await else {
        panic!("no Godot found — set BHIPPI_GODOT to the console build");
    };
    println!(
        "godot: cli={} gui={} ({})",
        install.cli().display(),
        install.gui().display(),
        install.version.raw
    );

    let workspace = workspace();
    let root = workspace.join("watch-play");
    write_project(&root, GAME_NAME, ProjectTemplate::ThirdPerson3D, false).expect("scaffold");

    // Everything already on this desktop that could pass the filter. The loop takes the same
    // snapshot; taking it here too is how the test proves the window it used was a new one.
    let filter = game_window_filter(GAME_NAME);
    let before: BTreeSet<u64> = find_windows(filter.clone())
        .await
        .map(|windows| windows.into_iter().map(|window| window.hwnd).collect())
        .unwrap_or_default();
    println!("windows matching before launch: {}", before.len());

    let plan = VisualPlaytestPlan::watch_play();
    println!(
        "plan: {} steps, {} planned frames, {} ms budget",
        plan.steps.len(),
        plan.planned_captures(),
        plan.max_ms
    );

    let result = run_visual_playtest(
        VisualLaunch {
            root: &root,
            gui_exe: install.gui(),
            game_name: GAME_NAME,
            stop: stop_channel(),
            lines: None,
            on_window: None,
        },
        &plan,
    )
    .await
    .expect("the visual playtest runs");

    println!(
        "stopped: {:?}{} after {} ms; window up at {} ms",
        result.stopped_reason,
        result
            .stopped_detail
            .as_ref()
            .map(|detail| format!(" ({detail})"))
            .unwrap_or_default(),
        result.elapsed_ms,
        result.window_ready_ms
    );
    for line in result.log_tail.iter().rev().take(6).rev() {
        println!("  godot | {line}");
    }

    // ── 1. frames ────────────────────────────────────────────────────────────────
    println!("captures: {}", result.captures.len());
    for capture in &result.captures {
        println!(
            "  frame step={:?} {}x{} via {:?} at {} ms (godot clock {:?}) — {}",
            capture.step_index,
            capture.width,
            capture.height,
            capture.method,
            capture.at_ms,
            capture.godot_time_ms,
            capture.note.as_deref().unwrap_or("(opening frame)")
        );
    }
    assert!(
        result.captures.len() >= MIN_CAPTURES,
        "expected at least {MIN_CAPTURES} frames, got {}: {:?}",
        result.captures.len(),
        result.stopped_detail
    );
    for capture in &result.captures {
        assert!(capture.width > 0 && capture.height > 0);
        assert!(
            capture.png_base64.starts_with("iVBORw0KGgo"),
            "every frame must be a real PNG"
        );
    }
    assert!(requires_vision(&result), "frames need a vision provider");

    // ── 2. the window ────────────────────────────────────────────────────────────
    println!(
        "window: hwnd={} pid={} class={} title={:?} client={}x{} dpi={:.2}",
        result.window.hwnd,
        result.window.process_id,
        result.window.class_name,
        result.window.title,
        result.window.rect.width,
        result.window.rect.height,
        result.window.dpi_scale
    );
    assert_eq!(result.window.class_name, "Engine");
    assert!(
        result.window.title.contains(GAME_NAME),
        "the window must be this game's, not another Godot window: {:?}",
        result.window.title
    );
    assert!(
        !before.contains(&result.window.hwnd),
        "the window must be one this run launched, not one already on the desktop"
    );
    assert!(
        !result.window.title.to_lowercase().contains("godot engine"),
        "the editor's window must never be picked up"
    );

    // ── 3. telemetry ─────────────────────────────────────────────────────────────
    let telemetry = result.telemetry.as_ref().expect("telemetry was requested");
    println!(
        "telemetry: {} samples, done={}, frames={:?}, malformed={}, tracked={:?}",
        telemetry.sample_count(),
        telemetry.done,
        telemetry.frames,
        telemetry.malformed_lines,
        telemetry.last_positions.keys().collect::<Vec<_>>()
    );
    assert!(
        telemetry.sample_count() >= MIN_SAMPLES,
        "expected at least {MIN_SAMPLES} telemetry samples, got {}",
        telemetry.sample_count()
    );
    assert!(
        telemetry.done,
        "the probe must write its done line — the game has to be asked to close, not killed"
    );
    assert_eq!(
        telemetry.malformed_lines, 0,
        "telemetry must be clean JSONL"
    );

    // ── 4. the pair ──────────────────────────────────────────────────────────────
    let evidence = result.evidence();
    println!(
        "evidence: {}/{} frames paired; faults: {:?}",
        evidence.paired_frames(),
        evidence.frames.len(),
        evidence.faults
    );
    for frame in &evidence.frames {
        match &frame.telemetry_sample {
            Some(sample) => println!(
                "  {} ms {:?} → godot frame {} at {} ms (skew {} ms) pos={:?} events={:?}",
                frame.at_ms,
                frame.note.as_deref().unwrap_or("(opening frame)"),
                sample.frame,
                sample.time_ms,
                sample.skew_ms,
                sample.tracked.first().and_then(|node| node.pos.clone()),
                sample.events.iter().map(|e| &e.name).collect::<Vec<_>>()
            ),
            None => println!(
                "  {} ms {:?} → NO SAMPLE (half a pair)",
                frame.at_ms,
                frame.note.as_deref().unwrap_or("(opening frame)")
            ),
        }
    }
    assert_eq!(
        evidence.frames.len(),
        result.captures.len(),
        "every frame is in the evidence, paired or not"
    );
    assert!(
        evidence.paired_frames() >= MIN_CAPTURES,
        "at least {MIN_CAPTURES} frames must have a telemetry sample behind them"
    );

    let positions: Vec<Vec<f64>> = evidence
        .frames
        .iter()
        .filter_map(|frame| frame.telemetry_sample.as_ref())
        .filter_map(|sample| sample.tracked.first().and_then(|node| node.pos.clone()))
        .collect();
    let (first, last) = (
        positions.first().expect("a first paired position"),
        positions.last().expect("a last paired position"),
    );
    let distance = travelled(first, last);
    println!("player travelled {distance:.3} between {first:?} and {last:?}");
    assert!(
        distance >= MIN_TRAVEL,
        "the keystrokes must have reached the game: the player moved only {distance:.3}"
    );

    // The capture method is the number this test exists to report: a GPU-composited Godot
    // window can answer PrintWindow with a blank frame, and knowing which path actually ran
    // is knowing what the frames can be trusted for.
    let methods: Vec<String> = result
        .captures
        .iter()
        .map(|capture| format!("{:?}", capture.method))
        .collect();
    println!("capture methods used: {methods:?}");

    assert_eq!(
        result.stopped_reason,
        VisualStopReason::Completed,
        "every step must have run: {:?}",
        result.stopped_detail
    );

    let _ignored = std::fs::remove_dir_all(&workspace);
}
