//! `bhippi-archetype@1` — the genre packs that fill in what the user did not say.
//!
//! An archetype is data, not a model call. It names the perspective, the player and camera
//! presets, the core loop, the level/HUD/rules presets, the actors, the art vocabulary and —
//! the part that matters most — **the questions that decide this genre**. A platformer asks
//! about lives versus checkpoints; a racer asks about laps versus a time trial. Those become
//! the plan card's open questions, so the build never guesses a decision the user cares about.
//!
//! Every capability id a pack names is checked against [`crate::intent::catalog`], so a typo
//! is a test failure rather than a scene that quietly misses a system.

use crate::error::{EngineError, Result};
use crate::game_spec::QuestionImpact;
use crate::intent::catalog;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// The only archetype format this build accepts.
pub const ARCHETYPE_FORMAT: &str = "bhippi-archetype@1";
/// Everything before the major version. A pack whose major differs is refused outright:
/// an unknown major means fields this build would silently drop.
pub const ARCHETYPE_FORMAT_STEM: &str = "bhippi-archetype@";

/// Requirement id for the player controller slot.
pub const REQ_PLAYER: &str = "req_player";
/// Requirement id for the camera slot.
pub const REQ_CAMERA: &str = "req_camera";
/// Requirement id for the level generator slot.
pub const REQ_LEVEL: &str = "req_level";
/// Requirement id for the HUD slot.
pub const REQ_HUD: &str = "req_hud";
/// Requirement id for the win/lose rules slot.
pub const REQ_RULES: &str = "req_rules";
/// Prefix for a per-actor-role requirement (`req_actor_rival`).
pub const REQ_ACTOR_PREFIX: &str = "req_actor_";
/// Prefix for a requirement derived from a preset id (`preset.ability.glide` ->
/// `req_ability_glide`).
pub const REQ_PRESET_PREFIX: &str = "req_";
/// Prefix for a requirement derived from a bare Godot class.
pub const REQ_NODE_PREFIX: &str = "req_node_";

/// The number of packs shipped in the binary.
pub const BUILTIN_ARCHETYPE_COUNT: usize = 10;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    TwoD,
    ThreeD,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Perspective {
    ThirdPerson,
    FirstPerson,
    TopDown,
    SideScroller,
    Isometric,
}

impl Perspective {
    /// The dimension a perspective implies when the prompt does not say. Isometric and
    /// top-down are genuinely used in both, so they imply nothing.
    #[must_use]
    pub fn implied_dimension(self) -> Option<Dimension> {
        match self {
            Self::ThirdPerson | Self::FirstPerson => Some(Dimension::ThreeD),
            Self::SideScroller => Some(Dimension::TwoD),
            Self::TopDown | Self::Isometric => None,
        }
    }
}

/// Which `GameSpec` list a requirement belongs in. Derived from the preset domain so the
/// same capability always lands in the same bucket across packs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SpecBucket {
    Mechanics,
    World,
    Actors,
    Ui,
}

/// The inclusive count range an actor role may be scaled to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CountRange {
    pub min: u16,
    pub max: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ActorTemplate {
    pub role: String,
    pub preset: String,
    pub count_default: u16,
    pub count_range: CountRange,
}

/// A decision this genre cannot sensibly default away.
///
/// `options` is a closed set; `default` — when present — is the answer a `High` question
/// takes when the user does not choose. `spec_from_draft` moves the default to the front of
/// `options` so the resulting [`crate::game_spec::OpenQuestion`], which has no default field
/// of its own, still carries it: **`options[0]` is the default** everywhere downstream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ArchetypeQuestion {
    pub id: String,
    pub question: String,
    pub impact: QuestionImpact,
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
    /// Requirement ids this answer reshapes. Every entry must be a requirement the pack
    /// actually produces.
    pub affects: Vec<String>,
}

/// A gameplay promise with the deterministic probes that prove it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct MechanicTemplate {
    pub id: String,
    pub promise: String,
    pub setup: Vec<String>,
    pub probes: Vec<String>,
    pub evidence: Vec<String>,
    /// Requirement ids the promise rests on. `GameSpec::validate` refuses a contract that
    /// cites nothing, so a template must name at least one.
    pub requires: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DefaultConstraints {
    pub platforms: Vec<String>,
    pub turn_tokens: u32,
    pub max_new_extensions: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Archetype {
    pub format: String,
    pub id: String,
    pub name: String,
    pub keywords: Vec<String>,
    pub dimension: Dimension,
    pub perspective: Perspective,
    pub player: String,
    pub camera: String,
    pub core_loop: Vec<String>,
    /// Capabilities beyond the five slots that this genre always needs.
    pub required: Vec<String>,
    /// Capabilities the genre often wants; the planner may add them, the compiler never does.
    #[serde(default)]
    pub optional: Vec<String>,
    pub level: String,
    pub hud: String,
    pub rules: String,
    #[serde(default)]
    pub actors: Vec<ActorTemplate>,
    pub art_vocabulary: Vec<String>,
    pub questions: Vec<ArchetypeQuestion>,
    pub defaults: DefaultConstraints,
    pub acceptance: Vec<MechanicTemplate>,
}

impl Archetype {
    pub fn parse(text: &str) -> Result<Self> {
        let pack: Self = serde_json::from_str(text).map_err(|error| {
            schema(
                format!("invalid archetype pack: {error}"),
                format!("Fix the JSON and keep format {ARCHETYPE_FORMAT}."),
            )
        })?;
        pack.validate()?;
        Ok(pack)
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_format()?;
        canonical_id(&self.id, "archetype")?;
        non_empty(&self.name, "archetype name")?;
        self.validate_keywords()?;
        self.validate_slots()?;
        self.validate_lists()?;
        self.validate_actors()?;
        let produced = self.requirement_ids();
        self.validate_questions(&produced)?;
        self.validate_acceptance(&produced)
    }

    fn validate_format(&self) -> Result<()> {
        if self.format == ARCHETYPE_FORMAT {
            return Ok(());
        }
        let major = self
            .format
            .strip_prefix(ARCHETYPE_FORMAT_STEM)
            .unwrap_or_default();
        Err(schema(
            format!("unsupported archetype format {:?}", self.format),
            if major.is_empty() {
                format!("Use {ARCHETYPE_FORMAT}.")
            } else {
                format!("This build reads {ARCHETYPE_FORMAT}; major {major} would drop fields, so it blocks.")
            },
        ))
    }

    fn validate_keywords(&self) -> Result<()> {
        if self.keywords.is_empty() {
            return Err(schema(
                format!("archetype {:?} has no keywords", self.id),
                "Name the words a person actually types for this genre.".to_owned(),
            ));
        }
        let mut seen = BTreeSet::new();
        for keyword in &self.keywords {
            let clean = keyword.trim();
            if clean.is_empty() || clean != keyword.to_lowercase() || !seen.insert(clean.to_owned())
            {
                return Err(schema(
                    format!(
                        "archetype {:?} keyword {keyword:?} is not canonical",
                        self.id
                    ),
                    "Use unique, lowercase, trimmed keywords.".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn validate_slots(&self) -> Result<()> {
        for (slot, id, domain) in [
            ("player", &self.player, "player"),
            ("camera", &self.camera, "camera"),
            ("level", &self.level, "level"),
            ("hud", &self.hud, "hud"),
            ("rules", &self.rules, "rules"),
        ] {
            known_capability(&self.id, id)?;
            if catalog::preset_domain(id) != Some(domain) {
                return Err(schema(
                    format!("archetype {:?} {slot} slot names {id:?}", self.id),
                    format!("Point the {slot} slot at a preset.{domain}.* card."),
                ));
            }
        }
        Ok(())
    }

    fn validate_lists(&self) -> Result<()> {
        if self.core_loop.is_empty() || self.core_loop.iter().any(|step| step.trim().is_empty()) {
            return Err(schema(
                format!("archetype {:?} has no core loop", self.id),
                "Describe the repeated player actions, one step per entry.".to_owned(),
            ));
        }
        if self.art_vocabulary.is_empty() {
            return Err(schema(
                format!("archetype {:?} has no art vocabulary", self.id),
                "List the art words this genre reads well in.".to_owned(),
            ));
        }
        let slots = [
            self.player.as_str(),
            self.camera.as_str(),
            self.level.as_str(),
            self.hud.as_str(),
            self.rules.as_str(),
        ];
        let mut seen = BTreeSet::new();
        for id in self.required.iter().chain(&self.optional) {
            known_capability(&self.id, id)?;
            if slots.contains(&id.as_str()) {
                return Err(schema(
                    format!("archetype {:?} repeats slot capability {id:?}", self.id),
                    "The five slots are already requirements; list only extras here.".to_owned(),
                ));
            }
            if !seen.insert(id.as_str()) {
                return Err(schema(
                    format!("archetype {:?} lists {id:?} twice", self.id),
                    "Name each capability once, either required or optional.".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn validate_actors(&self) -> Result<()> {
        let mut roles = BTreeSet::new();
        for actor in &self.actors {
            canonical_id(&actor.role, "actor role")?;
            if !roles.insert(actor.role.as_str()) {
                return Err(schema(
                    format!(
                        "archetype {:?} repeats actor role {:?}",
                        self.id, actor.role
                    ),
                    "Give every actor role one stable name.".to_owned(),
                ));
            }
            known_capability(&self.id, &actor.preset)?;
            if actor.count_range.min > actor.count_default
                || actor.count_default > actor.count_range.max
            {
                return Err(schema(
                    format!(
                        "archetype {:?} actor {:?} default {} is outside {}..={}",
                        self.id,
                        actor.role,
                        actor.count_default,
                        actor.count_range.min,
                        actor.count_range.max
                    ),
                    "Keep count_default inside count_range.".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn validate_questions(&self, produced: &BTreeSet<String>) -> Result<()> {
        if self.questions.is_empty() {
            return Err(schema(
                format!("archetype {:?} asks nothing", self.id),
                "Every genre has at least one High or Critical decision; name it.".to_owned(),
            ));
        }
        let mut ids = BTreeSet::new();
        for question in &self.questions {
            canonical_id(&question.id, "archetype question")?;
            non_empty(&question.question, "archetype question text")?;
            if !ids.insert(question.id.as_str()) {
                return Err(schema(
                    format!("archetype {:?} repeats question {:?}", self.id, question.id),
                    "Give every decision one stable id.".to_owned(),
                ));
            }
            let unique = question.options.iter().collect::<BTreeSet<_>>().len();
            if question.options.len() < 2
                || unique != question.options.len()
                || question.options.iter().any(|opt| opt.trim().is_empty())
            {
                return Err(schema(
                    format!(
                        "archetype {:?} question {:?} lacks options",
                        self.id, question.id
                    ),
                    "Offer at least two distinct, non-empty options.".to_owned(),
                ));
            }
            if let Some(default) = &question.default {
                if !question.options.contains(default) {
                    return Err(schema(
                        format!(
                            "archetype {:?} question {:?} defaults to {default:?}",
                            self.id, question.id
                        ),
                        "The default must be one of the listed options.".to_owned(),
                    ));
                }
            } else if question.impact == QuestionImpact::High {
                return Err(schema(
                    format!(
                        "archetype {:?} High question {:?} has no default",
                        self.id, question.id
                    ),
                    "High questions are answered by default and flagged; give the default."
                        .to_owned(),
                ));
            }
            if question.affects.is_empty()
                || question.affects.iter().any(|id| !produced.contains(id))
            {
                return Err(schema(
                    format!(
                        "archetype {:?} question {:?} affects nothing this pack builds",
                        self.id, question.id
                    ),
                    "Name requirement ids this pack produces, such as req_rules.".to_owned(),
                ));
            }
        }
        if !self.questions.iter().any(|question| {
            matches!(
                question.impact,
                QuestionImpact::High | QuestionImpact::Critical
            )
        }) {
            return Err(schema(
                format!("archetype {:?} has no material question", self.id),
                "At least one question must be High or Critical.".to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_acceptance(&self, produced: &BTreeSet<String>) -> Result<()> {
        if self.acceptance.is_empty() {
            return Err(schema(
                format!("archetype {:?} promises nothing testable", self.id),
                "Add at least one acceptance mechanic with probes and evidence.".to_owned(),
            ));
        }
        let mut ids = BTreeSet::new();
        for mechanic in &self.acceptance {
            canonical_id(&mechanic.id, "acceptance mechanic")?;
            non_empty(&mechanic.promise, "acceptance promise")?;
            if !ids.insert(mechanic.id.as_str()) {
                return Err(schema(
                    format!("archetype {:?} repeats mechanic {:?}", self.id, mechanic.id),
                    "Give every acceptance mechanic one stable id.".to_owned(),
                ));
            }
            if mechanic.setup.is_empty()
                || mechanic.probes.is_empty()
                || mechanic.evidence.is_empty()
                || mechanic
                    .setup
                    .iter()
                    .chain(&mechanic.probes)
                    .chain(&mechanic.evidence)
                    .any(|line| line.trim().is_empty())
            {
                return Err(schema(
                    format!(
                        "archetype {:?} mechanic {:?} is not testable",
                        self.id, mechanic.id
                    ),
                    "Provide setup, deterministic probes and expected evidence.".to_owned(),
                ));
            }
            if mechanic.requires.is_empty()
                || mechanic.requires.iter().any(|id| !produced.contains(id))
            {
                return Err(schema(
                    format!(
                        "archetype {:?} mechanic {:?} cites a requirement it does not build",
                        self.id, mechanic.id
                    ),
                    "Cite requirement ids this pack produces, such as req_player.".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Every requirement id `spec_from_draft` will emit for this pack, in a stable order.
    #[must_use]
    pub fn requirement_ids(&self) -> BTreeSet<String> {
        self.requirements()
            .into_iter()
            .map(|slot| slot.requirement_id)
            .collect()
    }

    /// The requirement rows this pack expands into: id, the capability behind it, the
    /// `GameSpec` list it belongs in, and the sentence the plan card shows.
    #[must_use]
    pub fn requirements(&self) -> Vec<ArchetypeRequirement> {
        let mut rows = vec![
            slot_row(REQ_PLAYER, &self.player, "Player control"),
            slot_row(REQ_CAMERA, &self.camera, "Camera"),
            slot_row(REQ_LEVEL, &self.level, "Level"),
            slot_row(REQ_HUD, &self.hud, "HUD"),
            slot_row(REQ_RULES, &self.rules, "Win and lose rules"),
        ];
        for actor in &self.actors {
            rows.push(ArchetypeRequirement {
                requirement_id: format!("{REQ_ACTOR_PREFIX}{}", actor.role),
                capability_id: actor.preset.clone(),
                bucket: SpecBucket::Actors,
                statement: format!(
                    "{} actors from {} ({} by default, {}..={})",
                    actor.role,
                    actor.preset,
                    actor.count_default,
                    actor.count_range.min,
                    actor.count_range.max
                ),
            });
        }
        for id in &self.required {
            rows.push(ArchetypeRequirement {
                requirement_id: requirement_id_for(id),
                capability_id: id.clone(),
                bucket: bucket_for(id),
                statement: format!("{} from {id}", purpose_of(id)),
            });
        }
        rows
    }

    /// The compiled-in pack with this id.
    #[must_use]
    pub fn find(id: &str) -> Option<&'static Self> {
        builtin().iter().find(|pack| pack.id == id)
    }
}

/// One expanded requirement row from a pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchetypeRequirement {
    pub requirement_id: String,
    pub capability_id: String,
    pub bucket: SpecBucket,
    pub statement: String,
}

fn slot_row(requirement_id: &str, capability_id: &str, label: &str) -> ArchetypeRequirement {
    ArchetypeRequirement {
        requirement_id: requirement_id.to_owned(),
        capability_id: capability_id.to_owned(),
        bucket: bucket_for(capability_id),
        statement: format!(
            "{label}: {} — {}",
            title_of(capability_id),
            purpose_of(capability_id)
        ),
    }
}

fn title_of(capability_id: &str) -> String {
    catalog::preset(capability_id)
        .map_or_else(|| capability_id.to_owned(), |card| card.title.to_owned())
}

fn purpose_of(capability_id: &str) -> String {
    catalog::preset(capability_id).map_or_else(
        || format!("a {capability_id} node"),
        |card| card.purpose.to_owned(),
    )
}

/// The requirement id a capability expands into. Dots and hyphens become underscores so the
/// result satisfies `GameSpec`'s canonical-id rule (lowercase, digits, underscore).
#[must_use]
pub fn requirement_id_for(capability_id: &str) -> String {
    match capability_id.strip_prefix(catalog::PRESET_PREFIX) {
        Some(rest) => format!(
            "{REQ_PRESET_PREFIX}{}",
            rest.replace(['.', '-'], "_").to_lowercase()
        ),
        None => format!("{REQ_NODE_PREFIX}{}", capability_id.to_lowercase()),
    }
}

/// Which `GameSpec` list a capability's requirement belongs in.
#[must_use]
pub fn bucket_for(capability_id: &str) -> SpecBucket {
    match catalog::preset_domain(capability_id) {
        Some("rules" | "ability" | "system") => SpecBucket::Mechanics,
        Some("player" | "camera" | "enemy" | "actor" | "tower") => SpecBucket::Actors,
        Some("hud") => SpecBucket::Ui,
        _ => SpecBucket::World,
    }
}

struct BuiltinPacks {
    packs: Vec<Archetype>,
    faults: Vec<String>,
}

const BUILTIN_SOURCES: [(&str, &str); BUILTIN_ARCHETYPE_COUNT] = [
    (
        "platformer_3d",
        include_str!("../../archetypes/platformer_3d.json"),
    ),
    (
        "platformer_2d",
        include_str!("../../archetypes/platformer_2d.json"),
    ),
    (
        "top_down_action",
        include_str!("../../archetypes/top_down_action.json"),
    ),
    ("fps_arena", include_str!("../../archetypes/fps_arena.json")),
    (
        "exploration",
        include_str!("../../archetypes/exploration.json"),
    ),
    (
        "racing_kart",
        include_str!("../../archetypes/racing_kart.json"),
    ),
    (
        "puzzle_physics",
        include_str!("../../archetypes/puzzle_physics.json"),
    ),
    (
        "tower_defense",
        include_str!("../../archetypes/tower_defense.json"),
    ),
    ("survival", include_str!("../../archetypes/survival.json")),
    (
        "endless_runner",
        include_str!("../../archetypes/endless_runner.json"),
    ),
];

fn loaded() -> &'static BuiltinPacks {
    static LOADED: OnceLock<BuiltinPacks> = OnceLock::new();
    LOADED.get_or_init(|| {
        let mut packs = Vec::with_capacity(BUILTIN_ARCHETYPE_COUNT);
        let mut faults = Vec::new();
        for (name, source) in BUILTIN_SOURCES {
            match Archetype::parse(source) {
                Ok(pack) if pack.id == name => packs.push(pack),
                Ok(pack) => faults.push(format!("{name}.json declares id {:?}", pack.id)),
                Err(error) => faults.push(format!("{name}.json: {error}")),
            }
        }
        packs.sort_by(|left, right| left.id.cmp(&right.id));
        BuiltinPacks { packs, faults }
    })
}

/// The packs compiled into this build, parsed and validated once, sorted by id.
///
/// A pack that fails to parse is reported by [`builtin_faults`] rather than panicking, so a
/// bad edit shows up as a failing test and never as a crashed editor.
#[must_use]
pub fn builtin() -> &'static [Archetype] {
    &loaded().packs
}

/// Why a compiled-in pack was dropped. Empty in a healthy build; asserted empty by tests.
#[must_use]
pub fn builtin_faults() -> &'static [String] {
    &loaded().faults
}

fn known_capability(pack: &str, id: &str) -> Result<()> {
    if catalog::is_known_id(id) {
        Ok(())
    } else {
        Err(schema(
            format!("archetype {pack:?} names unknown capability {id:?}"),
            "Use a preset id from intent::catalog::presets() or a catalogued Godot class."
                .to_owned(),
        ))
    }
}

fn canonical_id(id: &str, label: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(schema(
            format!("{label} id {id:?} is not canonical"),
            "Use lowercase letters, digits and underscores; GameSpec ids allow nothing else."
                .to_owned(),
        ))
    }
}

fn non_empty(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(schema(
            format!("{label} must not be empty"),
            format!("Provide a concrete {label}."),
        ))
    } else {
        Ok(())
    }
}

fn schema(message: String, hint: String) -> EngineError {
    EngineError::Schema(message, Some(hint))
}

#[cfg(test)]
mod tests {
    use super::{
        bucket_for, builtin, builtin_faults, requirement_id_for, Archetype, Dimension, Perspective,
        SpecBucket, ARCHETYPE_FORMAT, BUILTIN_ARCHETYPE_COUNT,
    };
    use crate::game_spec::QuestionImpact;
    use crate::intent::catalog;
    use std::collections::BTreeSet;

    #[test]
    fn every_builtin_pack_parses_and_validates() {
        assert_eq!(builtin_faults(), &[] as &[String]);
        assert_eq!(builtin().len(), BUILTIN_ARCHETYPE_COUNT);
        for pack in builtin() {
            pack.validate().expect("builtin pack validates");
        }
    }

    #[test]
    fn the_ten_named_packs_are_present_and_uniquely_identified() {
        let ids = builtin().iter().map(|p| p.id.as_str()).collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "endless_runner",
                "exploration",
                "fps_arena",
                "platformer_2d",
                "platformer_3d",
                "puzzle_physics",
                "racing_kart",
                "survival",
                "top_down_action",
                "tower_defense",
            ]
        );
    }

    #[test]
    fn every_id_a_pack_names_exists_in_the_catalogue() {
        for pack in builtin() {
            for id in [
                &pack.player,
                &pack.camera,
                &pack.level,
                &pack.hud,
                &pack.rules,
            ]
            .into_iter()
            .chain(&pack.required)
            .chain(&pack.optional)
            .chain(pack.actors.iter().map(|actor| &actor.preset))
            {
                assert!(
                    catalog::is_known_id(id),
                    "{} names uncatalogued {id}",
                    pack.id
                );
            }
        }
    }

    #[test]
    fn every_pack_asks_a_material_question_and_promises_a_testable_mechanic() {
        for pack in builtin() {
            assert!(
                pack.questions
                    .iter()
                    .any(|q| matches!(q.impact, QuestionImpact::High | QuestionImpact::Critical)),
                "{} asks nothing material",
                pack.id
            );
            assert!(
                pack.questions
                    .iter()
                    .any(|q| q.impact == QuestionImpact::Critical),
                "{} has no Critical decision",
                pack.id
            );
            assert!(!pack.acceptance.is_empty(), "{} promises nothing", pack.id);
        }
    }

    #[test]
    fn keywords_are_unique_across_packs_so_matching_cannot_tie_on_one_word() {
        let mut owner = std::collections::BTreeMap::new();
        for pack in builtin() {
            for keyword in &pack.keywords {
                if let Some(other) = owner.insert(keyword.clone(), pack.id.clone()) {
                    panic!("{keyword:?} is claimed by both {other} and {}", pack.id);
                }
            }
        }
    }

    #[test]
    fn requirement_rows_are_unique_and_bucketed_by_domain() {
        for pack in builtin() {
            let rows = pack.requirements();
            let ids = rows
                .iter()
                .map(|row| row.requirement_id.as_str())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                ids.len(),
                rows.len(),
                "{} repeats a requirement id",
                pack.id
            );
            assert!(rows.iter().any(|row| row.bucket == SpecBucket::Ui));
            assert!(rows.iter().any(|row| row.bucket == SpecBucket::Mechanics));
        }
        assert_eq!(bucket_for("preset.hud.lap_timer"), SpecBucket::Ui);
        assert_eq!(bucket_for("preset.ability.glide"), SpecBucket::Mechanics);
        assert_eq!(bucket_for("preset.enemy.turret"), SpecBucket::Actors);
        assert_eq!(bucket_for("preset.weather.rain"), SpecBucket::World);
        assert_eq!(bucket_for("DirectionalLight3D"), SpecBucket::World);
    }

    #[test]
    fn requirement_ids_stay_canonical_for_gamespec() {
        assert_eq!(
            requirement_id_for("preset.ability.glide"),
            "req_ability_glide"
        );
        assert_eq!(
            requirement_id_for("preset.system.day_night"),
            "req_system_day_night"
        );
        assert_eq!(
            requirement_id_for("DirectionalLight3D"),
            "req_node_directionallight3d"
        );
        for pack in builtin() {
            for id in pack.requirement_ids() {
                assert!(
                    id.chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
                    "{id} is not a canonical GameSpec id"
                );
            }
        }
    }

    #[test]
    fn an_unknown_major_format_blocks_with_a_hint() {
        let source = r#"{"format":"bhippi-archetype@2","id":"x","name":"X","keywords":["x"],
            "dimension":"three_d","perspective":"third_person","player":"preset.player.fps",
            "camera":"preset.camera.first_person","core_loop":["shoot"],"required":[],
            "level":"preset.level.arena","hud":"preset.hud.ammo_health",
            "rules":"preset.rules.frag_limit","art_vocabulary":["neon"],
            "questions":[],"defaults":{"platforms":["windows"],"turn_tokens":1,
            "max_new_extensions":0},"acceptance":[]}"#;
        let error = Archetype::parse(source).expect_err("major 2 blocks");
        assert!(error.to_string().contains("bhippi-archetype@2"));
        assert!(error.hint().is_some_and(|hint| hint.contains("major 2")));
    }

    #[test]
    fn an_uncatalogued_capability_blocks() {
        let source = format!(
            r#"{{"format":"{ARCHETYPE_FORMAT}","id":"x","name":"X","keywords":["x"],
            "dimension":"three_d","perspective":"third_person","player":"preset.player.hovercraft",
            "camera":"preset.camera.first_person","core_loop":["shoot"],"required":[],
            "level":"preset.level.arena","hud":"preset.hud.ammo_health",
            "rules":"preset.rules.frag_limit","art_vocabulary":["neon"],
            "questions":[],"defaults":{{"platforms":["windows"],"turn_tokens":1,
            "max_new_extensions":0}},"acceptance":[]}}"#
        );
        let error = Archetype::parse(&source).expect_err("uncatalogued preset blocks");
        assert!(error.to_string().contains("preset.player.hovercraft"));
    }

    #[test]
    fn perspective_implies_a_dimension_only_where_it_really_does() {
        assert_eq!(
            Perspective::ThirdPerson.implied_dimension(),
            Some(Dimension::ThreeD)
        );
        assert_eq!(
            Perspective::SideScroller.implied_dimension(),
            Some(Dimension::TwoD)
        );
        assert_eq!(Perspective::TopDown.implied_dimension(), None);
        assert_eq!(Perspective::Isometric.implied_dimension(), None);
    }
}
