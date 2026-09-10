//! The no-model fast path: "make the glide 20% longer" without spending a single token.
//!
//! Most follow-ups in a game studio are parameter edits on something that already exists.
//! This module matches `<qualifier> <noun>` (in either order) against the noun table in
//! [`crate::intent::catalog`] and the nodes actually present in the project, and proposes one
//! bounded change with a confidence.
//!
//! The rule that keeps it honest is **never guess a node**. If exactly one node exposes the
//! knob, the proposal names it and applies. If several do, the proposal names none of them,
//! lists the candidates and drops to confirm level. If none do, there is no proposal and the
//! turn goes to a model — a wrong silent edit costs far more than a model call.
//!
//! Three rules protect that promise, in this order:
//!
//! 1. **A node the user named wins, and only it.** "the Lamp's light_energy" edits `Lamp`,
//!    never whichever other node happens to own the class the noun table lists. A named node
//!    that cannot carry the knob is a model turn, not a quiet fallback onto one that can.
//! 2. **One change per turn.** A sentence asking for two things ("light_energy to 4 and
//!    light_color to orange") goes to a model whole. Half a request applied silently is the
//!    worst outcome this module can produce.
//! 3. **Several possible targets is a question, not a pick.** Class matching walks the whole
//!    family a property lives on (`CLASS_FAMILIES`), so a scene holding an `OmniLight3D` and
//!    a `DirectionalLight3D` asks which light was meant instead of taking the one whose class
//!    the noun table happens to name.

use crate::intent::catalog::{self, NounEntry};
use crate::intent::draft::{find_phrase, flatten};
use serde::{Deserialize, Serialize};
use specta::Type;

/// The step a bare qualifier ("higher", "slower") means when no number is given.
pub const DEFAULT_STEP_FACTOR: f64 = 1.2;
/// The step "slightly" or "a bit" means.
pub const SMALL_STEP_FACTOR: f64 = 1.1;
/// The step "much" or "way" means.
pub const LARGE_STEP_FACTOR: f64 = 1.5;

/// At or above this the edit applies straight away behind an Undo toast.
pub const FAST_PATH_APPLY_BPS: u16 = 9_000;
/// At or above this the edit is offered as a confirm chip. Below it there is no proposal.
pub const FAST_PATH_CONFIRM_BPS: u16 = 6_000;

/// Exactly one node in the project exposes the knob.
pub const CONFIDENCE_UNIQUE_NODE_BPS: u16 = 9_500;
/// The knob lives on a Godot class and exactly one node has that class.
pub const CONFIDENCE_UNIQUE_CLASS_BPS: u16 = 9_200;
/// No node exposes it, but exactly one preset in the project owns the knob.
pub const CONFIDENCE_UNIQUE_PRESET_BPS: u16 = 9_200;
/// Several things expose the knob: offer, list them, and let the user choose.
pub const CONFIDENCE_SEVERAL_CANDIDATES_BPS: u16 = 7_000;
/// Charged when the qualifier carries no number ("faster" rather than "20% faster").
pub const VAGUE_QUALIFIER_PENALTY_BPS: u16 = 300;
/// How many candidates a disambiguating proposal may list.
pub const MAX_FAST_PATH_CANDIDATES: usize = 8;

/// One script variable already on a node, with the value it holds now.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ScriptVar {
    pub name: String,
    pub value: f64,
}

/// What the caller knows about one node in the open scene.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct NodeSummary {
    pub path: String,
    pub class: String,
    #[serde(default)]
    pub script_vars: Vec<ScriptVar>,
}

/// Everything the fast path is allowed to look at. No registry, no scene document, no I/O.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct FastPathContext {
    #[serde(default)]
    pub presets_in_project: Vec<String>,
    #[serde(default)]
    pub nodes: Vec<NodeSummary>,
}

/// What the edit lands on. Exactly one of `node_path` and `preset_id` is set when the
/// proposal is actionable; both are `None` when [`FastPathProposal::needs_choice`] is true.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Target {
    pub node_path: Option<String>,
    pub preset_id: Option<String>,
    pub property: String,
}

/// The subset of `.tscn` values a parameter edit can produce.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TscnValueLite {
    Number { value: f64 },
    Bool { value: bool },
    Text { value: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FastPathOp {
    Multiply { factor: f64 },
    Add { amount: f64 },
    Set { value: TscnValueLite },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct FastPathProposal {
    pub target: Target,
    pub op: FastPathOp,
    pub confidence_bps: u16,
    pub rationale: String,
    /// The sentence the Undo toast or confirm chip shows.
    pub label: String,
    /// Node paths or preset ids when the knob is not unique. Empty on a settled proposal.
    pub candidates: Vec<String>,
}

impl FastPathProposal {
    /// Whether the caller must pick a target before this can be applied.
    #[must_use]
    pub fn needs_choice(&self) -> bool {
        self.target.node_path.is_none() && self.target.preset_id.is_none()
    }

    /// Whether the edit may be applied straight away behind an Undo toast.
    #[must_use]
    pub fn applies_without_asking(&self) -> bool {
        self.confidence_bps >= FAST_PATH_APPLY_BPS && !self.needs_choice()
    }
}

/// Propose one parameter edit, or `None` when the utterance is not one.
#[must_use]
pub fn propose(utterance: &str, ctx: &FastPathContext) -> Option<FastPathProposal> {
    let tokens = scan(utterance);
    if tokens.is_empty() {
        return None;
    }
    let padded = format!(" {} ", flatten(&tokens.join(" ")));
    let (entry, matched_word) = match_noun(&padded)?;
    // Rule 2: a sentence that asks for two things goes to a model whole, never half applied.
    if asks_for_more_than_one_change(utterance, entry.property) {
        return None;
    }
    let intent = read_op(&padded, &tokens, entry)?;
    let named = named_nodes(utterance, ctx);
    let resolved = resolve(ctx, &intent.property, &named)?;

    let mut confidence_bps = resolved.confidence_bps;
    if intent.vague {
        confidence_bps = confidence_bps.saturating_sub(VAGUE_QUALIFIER_PENALTY_BPS);
    }
    if confidence_bps < FAST_PATH_CONFIRM_BPS {
        return None;
    }

    let label = label_for(&intent, resolved.current);
    Some(FastPathProposal {
        target: Target {
            node_path: resolved.node_path,
            preset_id: resolved.preset_id,
            property: intent.property.clone(),
        },
        op: intent.op,
        confidence_bps,
        rationale: format!(
            "{matched_word:?} means {} · {}",
            intent.property, resolved.why
        ),
        label,
        candidates: resolved.candidates,
    })
}

struct OpIntent {
    property: String,
    op: FastPathOp,
    /// A qualifier with no number attached, which costs a little confidence.
    vague: bool,
}

struct Resolved {
    node_path: Option<String>,
    preset_id: Option<String>,
    candidates: Vec<String>,
    confidence_bps: u16,
    current: Option<f64>,
    why: String,
}

/// The longest noun phrase in the utterance, and the words that spelled it.
fn match_noun(padded: &str) -> Option<(&'static NounEntry, String)> {
    let mut best: Option<(&'static NounEntry, String, usize)> = None;
    for entry in catalog::nouns() {
        for word in entry.words {
            if find_phrase(padded, word).is_none() {
                continue;
            }
            let width = flatten(word).len();
            if best.as_ref().is_none_or(|(_, _, held)| width > *held) {
                best = Some((entry, (*word).to_owned(), width));
            }
        }
    }
    best.map(|(entry, word, _)| (entry, word))
}

/// Words that mean "more of it". Module level because the one-change guard reads them too.
const INCREASE: &[&str] = &[
    "higher", "bigger", "faster", "brighter", "longer", "more", "stronger", "heavier", "louder",
    "further", "harder", "increase", "raise", "boost", "up",
];
/// Words that mean "less of it".
const DECREASE: &[&str] = &[
    "lower", "smaller", "slower", "darker", "shorter", "less", "fewer", "weaker", "quieter",
    "softer", "easier", "reduce", "decrease", "cut", "down",
];

fn read_op(padded: &str, tokens: &[String], entry: &NounEntry) -> Option<OpIntent> {
    if let Some(op) = read_toggle(padded, tokens, entry) {
        return Some(op);
    }
    if let Some(op) = read_choice(padded, entry) {
        return Some(op);
    }
    if let Some(value) = read_set_value(tokens) {
        return Some(OpIntent {
            property: entry.property.to_owned(),
            op: FastPathOp::Set {
                value: TscnValueLite::Number { value },
            },
            vague: false,
        });
    }
    if let Some(amount) = read_add_amount(tokens, INCREASE, DECREASE) {
        return Some(OpIntent {
            property: entry.property.to_owned(),
            op: FastPathOp::Add { amount },
            vague: false,
        });
    }

    let up = INCREASE
        .iter()
        .any(|word| find_phrase(padded, word).is_some());
    let down = DECREASE
        .iter()
        .any(|word| find_phrase(padded, word).is_some());
    if let Some(percent) = read_percent(tokens) {
        let factor = if down && !up {
            1.0 - percent / 100.0
        } else if up {
            1.0 + percent / 100.0
        } else {
            return None;
        };
        if factor <= 0.0 {
            return None;
        }
        return Some(OpIntent {
            property: entry.property.to_owned(),
            op: FastPathOp::Multiply { factor },
            vague: false,
        });
    }
    if let Some(factor) = read_scale_word(padded) {
        return Some(OpIntent {
            property: entry.property.to_owned(),
            op: FastPathOp::Multiply { factor },
            vague: false,
        });
    }
    if up == down {
        return None;
    }
    let step = if find_phrase(padded, "slightly").is_some()
        || find_phrase(padded, "a bit").is_some()
        || find_phrase(padded, "a little").is_some()
    {
        SMALL_STEP_FACTOR
    } else if find_phrase(padded, "much").is_some()
        || find_phrase(padded, "way").is_some()
        || find_phrase(padded, "a lot").is_some()
    {
        LARGE_STEP_FACTOR
    } else {
        DEFAULT_STEP_FACTOR
    };
    Some(OpIntent {
        property: entry.property.to_owned(),
        op: FastPathOp::Multiply {
            factor: if up { step } else { 1.0 / step },
        },
        vague: true,
    })
}

/// On/off only when the utterance is really switching something: it ends in "on"/"off", or
/// it carries a switching verb. Without that guard `"thicker fog on the ridge"` would read
/// as `fog_enabled = true`.
fn read_toggle(padded: &str, tokens: &[String], entry: &NounEntry) -> Option<OpIntent> {
    let property = entry.toggle_property()?;
    let has = |word: &str| find_phrase(padded, word).is_some();
    let switching = has("turn") || has("switch");
    let ends_with = |word: &str| tokens.last().is_some_and(|last| last == word);
    let on = ends_with("on") || has("enable") || has("enabled") || (switching && has("on"));
    let off = ends_with("off") || has("disable") || has("disabled") || (switching && has("off"));
    if on == off {
        return None;
    }
    Some(OpIntent {
        property: property.to_owned(),
        op: FastPathOp::Set {
            value: TscnValueLite::Bool { value: on },
        },
        vague: false,
    })
}

fn read_choice(padded: &str, entry: &NounEntry) -> Option<OpIntent> {
    let options = entry.choice?;
    let chosen = options
        .iter()
        .find(|option| find_phrase(padded, option).is_some())?;
    Some(OpIntent {
        property: entry.property.to_owned(),
        op: FastPathOp::Set {
            value: TscnValueLite::Text {
                value: (*chosen).to_owned(),
            },
        },
        vague: false,
    })
}

/// `"to 8"` or `"to 5.5"`. Requires the preposition so `"8 enemies"` is not read as a set.
fn read_set_value(tokens: &[String]) -> Option<f64> {
    tokens.windows(2).find_map(|pair| {
        (pair[0] == "to" || pair[0] == "at")
            .then(|| parse_number(&pair[1]))
            .flatten()
    })
}

/// `"add 2"`, `"2 more"`, `"3 fewer"`.
fn read_add_amount(tokens: &[String], increase: &[&str], decrease: &[&str]) -> Option<f64> {
    for pair in tokens.windows(2) {
        if pair[0] == "add" || pair[0] == "plus" {
            if let Some(value) = parse_number(&pair[1]) {
                return Some(value);
            }
        }
        let Some(value) = parse_number(&pair[0]) else {
            continue;
        };
        if increase.contains(&pair[1].as_str()) {
            return Some(value);
        }
        if decrease.contains(&pair[1].as_str()) {
            return Some(-value);
        }
    }
    None
}

fn read_percent(tokens: &[String]) -> Option<f64> {
    tokens
        .iter()
        .find_map(|token| token.strip_suffix('%').and_then(parse_number))
}

fn read_scale_word(padded: &str) -> Option<f64> {
    for (word, factor) in [
        ("half", 0.5),
        ("double", 2.0),
        ("twice", 2.0),
        ("triple", 3.0),
        ("quarter", 0.25),
    ] {
        if find_phrase(padded, word).is_some() {
            return Some(factor);
        }
    }
    None
}

/// The separators a second request hides behind.
const CHANGE_SEPARATORS: &[&str] = &[" and ", " then ", "; ", ", "];
/// The private marker clause splitting leaves behind. It never occurs in real prose.
const SEPARATOR_MARK: &str = "\u{1}";

/// Whether the utterance asks for more than one change.
///
/// Only the clause holding the winning noun may carry an instruction. Anything recognisable
/// after a conjunction — a second noun, or a qualifier of its own — means applying this
/// proposal would deliver half of what was asked, so the whole turn belongs to a model.
///
/// Deliberately *not* "a second noun anywhere in the sentence": `"make the enemy damage 20%
/// higher"` matches both `enemy` and `damage` out of a single noun phrase, and is one change.
fn asks_for_more_than_one_change(utterance: &str, property: &str) -> bool {
    let clauses = clauses_of(utterance);
    if clauses.len() < 2 {
        return false;
    }
    let mut own_clause_seen = false;
    for clause in &clauses {
        let tokens = scan(clause);
        if tokens.is_empty() {
            continue;
        }
        let padded = format!(" {} ", flatten(&tokens.join(" ")));
        let noun = match_noun(&padded).map(|(entry, _)| entry.property);
        if !own_clause_seen && noun == Some(property) {
            own_clause_seen = true;
            continue;
        }
        if noun.is_some() || carries_a_qualifier(&padded, &tokens) {
            return true;
        }
    }
    false
}

/// The utterance split on every separator in [`CHANGE_SEPARATORS`]: lowercased with runs of
/// whitespace collapsed, but punctuation kept, because the separators are punctuation.
fn clauses_of(utterance: &str) -> Vec<String> {
    let mut text = utterance
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for separator in CHANGE_SEPARATORS {
        text = text.replace(separator, SEPARATOR_MARK);
    }
    text.split(SEPARATOR_MARK).map(str::to_owned).collect()
}

/// A clause carrying an instruction of its own: a direction word, or its own value.
fn carries_a_qualifier(padded: &str, tokens: &[String]) -> bool {
    INCREASE
        .iter()
        .chain(DECREASE)
        .any(|word| find_phrase(padded, word).is_some())
        || read_set_value(tokens).is_some()
}

/// The nodes the utterance named outright, by node name or by full path.
///
/// A name is one word of the utterance with its punctuation and any possessive `'s` removed,
/// compared case-insensitively. Middle path segments deliberately do not count: `"the
/// enemies"` in `/root/Game/Enemies/Chaser` names a folder, not a node.
fn named_nodes<'a>(utterance: &str, ctx: &'a FastPathContext) -> Vec<&'a NodeSummary> {
    let mentions = mentions_of(utterance);
    if mentions.is_empty() {
        return Vec::new();
    }
    let mut named = ctx
        .nodes
        .iter()
        .filter(|node| {
            let name = node
                .path
                .rsplit('/')
                .next()
                .unwrap_or(node.path.as_str())
                .to_lowercase();
            let path = node.path.to_lowercase();
            mentions.iter().any(|word| *word == name || *word == path)
        })
        .collect::<Vec<_>>();
    named.sort_by(|left, right| left.path.cmp(&right.path));
    named.dedup_by(|left, right| left.path == right.path);
    named
}

/// Every word of the utterance as a node name might have been written: case folded, the
/// possessive dropped, surrounding punctuation stripped. `/` and `_` survive, so a full node
/// path and an underscored node name each stay one word.
fn mentions_of(utterance: &str) -> Vec<String> {
    let edge = |ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '/';
    utterance
        .split_whitespace()
        .filter_map(|word| {
            let held = word.trim_matches(|ch: char| edge(ch) && ch != '\'' && ch != '\u{2019}');
            let bare = held
                .strip_suffix("'s")
                .or_else(|| held.strip_suffix("\u{2019}s"))
                .unwrap_or(held)
                .trim_matches(edge);
            (!bare.is_empty()).then(|| bare.to_lowercase())
        })
        .collect()
}

/// Godot 4 classes that answer to the same property. Written out here because neither this
/// crate nor the catalogue models Godot's class hierarchy: `light_energy` is declared on
/// `Light3D`, so every light carries it, and `volume_db` is declared on `AudioStreamPlayer`.
///
/// Physics bodies are deliberately not a family. `mass` belongs to `RigidBody3D` and a
/// `StaticBody3D` has no such knob, so grouping the bodies would put back exactly the
/// wrong-node edit this table exists to prevent.
const CLASS_FAMILIES: &[&[&str]] = &[
    &["DirectionalLight3D", "OmniLight3D", "SpotLight3D"],
    &["DirectionalLight2D", "PointLight2D"],
    &["Camera2D", "Camera3D"],
    &[
        "AudioStreamPlayer",
        "AudioStreamPlayer2D",
        "AudioStreamPlayer3D",
    ],
];

/// Whether two class names are the same class, or two members of one family.
fn same_family(class: &str, other: &str) -> bool {
    class == other
        || CLASS_FAMILIES
            .iter()
            .any(|family| family.contains(&class) && family.contains(&other))
}

/// Whether this node could plausibly hold the knob: it exposes it as a script variable, or
/// its class is the one the noun table names for it, or a sibling in that class's family.
fn carries(node: &NodeSummary, property: &str) -> bool {
    node.script_vars.iter().any(|var| var.name == property)
        || class_for(property).is_some_and(|class| same_family(class, &node.class))
}

fn script_value(node: &NodeSummary, property: &str) -> Option<f64> {
    node.script_vars
        .iter()
        .find(|var| var.name == property)
        .map(|var| var.value)
}

/// Rule 1. A node the user named is the only candidate there is: it takes the edit when it
/// can carry the knob, the turn goes to a model when it cannot, and the user is asked when
/// the name fits more than one node.
fn resolve_named(named: &[&NodeSummary], property: &str) -> Option<Resolved> {
    let able = named
        .iter()
        .copied()
        .filter(|node| carries(node, property))
        .collect::<Vec<_>>();
    match able.as_slice() {
        // The user named something that has no such knob. Quietly editing a node that does
        // have it is the silent wrong edit this rule exists to stop.
        [] => None,
        [node] => Some(Resolved {
            node_path: Some(node.path.clone()),
            preset_id: None,
            candidates: Vec::new(),
            // Naming a node settles *which* node, not how sure the knob reading is, so the
            // confidence stays the one the knob itself earns.
            confidence_bps: if script_value(node, property).is_some() {
                CONFIDENCE_UNIQUE_NODE_BPS
            } else {
                CONFIDENCE_UNIQUE_CLASS_BPS
            },
            current: script_value(node, property),
            why: format!("you named {}", node.path),
        }),
        nodes => Some(several(
            nodes.iter().map(|node| node.path.clone()).collect(),
            "more than one node answers to the name you used",
        )),
    }
}

fn resolve(ctx: &FastPathContext, property: &str, named: &[&NodeSummary]) -> Option<Resolved> {
    if !named.is_empty() {
        return resolve_named(named, property);
    }

    let mut by_var = ctx
        .nodes
        .iter()
        .filter(|node| node.script_vars.iter().any(|var| var.name == property))
        .collect::<Vec<_>>();
    by_var.sort_by(|left, right| left.path.cmp(&right.path));
    if by_var.len() == 1 {
        let node = by_var[0];
        return Some(Resolved {
            node_path: Some(node.path.clone()),
            preset_id: None,
            candidates: Vec::new(),
            confidence_bps: CONFIDENCE_UNIQUE_NODE_BPS,
            current: node
                .script_vars
                .iter()
                .find(|var| var.name == property)
                .map(|var| var.value),
            why: format!("only {} exposes it", node.path),
        });
    }
    if by_var.len() > 1 {
        return Some(several(
            by_var.iter().map(|node| node.path.clone()).collect(),
            "several nodes expose it",
        ));
    }

    // Rule 3: the whole family a property lives on is in scope, so a scene with one light of
    // the "wrong" class still resolves, and a scene with two lights asks which one.
    if let Some(class) = class_for(property) {
        let mut by_class = ctx
            .nodes
            .iter()
            .filter(|node| same_family(class, &node.class))
            .collect::<Vec<_>>();
        by_class.sort_by(|left, right| left.path.cmp(&right.path));
        if let [node] = by_class.as_slice() {
            return Some(Resolved {
                node_path: Some(node.path.clone()),
                preset_id: None,
                candidates: Vec::new(),
                confidence_bps: CONFIDENCE_UNIQUE_CLASS_BPS,
                current: None,
                why: format!("{} lives on {} alone", node.class, node.path),
            });
        }
        if by_class.len() > 1 {
            return Some(several(
                by_class.iter().map(|node| node.path.clone()).collect(),
                &format!("several nodes could be the {class} that was meant"),
            ));
        }
    }

    let mut owners = catalog::presets_exposing(property)
        .into_iter()
        .map(|card| card.id.to_owned())
        .filter(|id| ctx.presets_in_project.contains(id))
        .collect::<Vec<_>>();
    owners.sort();
    match owners.len() {
        0 => None,
        1 => Some(Resolved {
            node_path: None,
            preset_id: owners.first().cloned(),
            candidates: Vec::new(),
            confidence_bps: CONFIDENCE_UNIQUE_PRESET_BPS,
            current: owners
                .first()
                .and_then(|id| catalog::preset_property(id, property))
                .and_then(|spec| parse_number(spec.default)),
            why: format!(
                "no node exposes it, but {} does",
                owners.first().map_or("", String::as_str)
            ),
        }),
        _ => Some(several(owners, "several presets own it")),
    }
}

fn several(mut candidates: Vec<String>, why: &str) -> Resolved {
    candidates.sort();
    candidates.dedup();
    candidates.truncate(MAX_FAST_PATH_CANDIDATES);
    Resolved {
        node_path: None,
        preset_id: None,
        candidates,
        confidence_bps: CONFIDENCE_SEVERAL_CANDIDATES_BPS,
        current: None,
        why: why.to_owned(),
    }
}

/// The Godot class a property belongs to, when the noun table says it is a node property
/// rather than a Bhippi script variable.
fn class_for(property: &str) -> Option<&'static str> {
    catalog::nouns()
        .iter()
        .find(|entry| entry.property == property)
        .and_then(|entry| entry.node_class)
}

fn label_for(intent: &OpIntent, current: Option<f64>) -> String {
    let property = &intent.property;
    match (&intent.op, current) {
        (FastPathOp::Multiply { factor }, Some(now)) => format!(
            "Set {property} {} → {}",
            format_number(now),
            format_number(now * factor)
        ),
        (FastPathOp::Multiply { factor }, None) => {
            format!("Multiply {property} by {}", format_number(*factor))
        }
        (FastPathOp::Add { amount }, Some(now)) => format!(
            "Set {property} {} → {}",
            format_number(now),
            format_number(now + amount)
        ),
        (FastPathOp::Add { amount }, None) if *amount < 0.0 => {
            format!("Subtract {} from {property}", format_number(-amount))
        }
        (FastPathOp::Add { amount }, None) => {
            format!("Add {} to {property}", format_number(*amount))
        }
        (FastPathOp::Set { value }, Some(now)) => match value {
            TscnValueLite::Number { value } => format!(
                "Set {property} {} → {}",
                format_number(now),
                format_number(*value)
            ),
            other => format!("Set {property} to {}", value_text(other)),
        },
        (FastPathOp::Set { value }, None) => {
            format!("Set {property} to {}", value_text(value))
        }
    }
}

fn value_text(value: &TscnValueLite) -> String {
    match value {
        TscnValueLite::Number { value } => format_number(*value),
        TscnValueLite::Bool { value } => (if *value { "on" } else { "off" }).to_owned(),
        TscnValueLite::Text { value } => value.clone(),
    }
}

/// Two decimals, trailing zeros trimmed. Fixed precision keeps the toast text identical on
/// every platform, which matters because the fixtures compare it byte for byte.
#[must_use]
pub fn format_number(value: f64) -> String {
    let mut text = format!("{:.2}", (value * 100.0).round() / 100.0);
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    text
}

fn parse_number(token: &str) -> Option<f64> {
    let value = token.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

/// Like the draft tokeniser but numeric: `20%` and `5.5` survive as single tokens, because
/// the fast path reads magnitudes and the draft pass does not.
fn scan(utterance: &str) -> Vec<String> {
    let chars = utterance.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut current = String::new();
    for (index, ch) in chars.iter().enumerate() {
        let keep = ch.is_alphanumeric()
            || *ch == '-'
            || (*ch == '.'
                && current.ends_with(|last: char| last.is_ascii_digit())
                && chars.get(index + 1).is_some_and(char::is_ascii_digit))
            || (*ch == '%' && current.ends_with(|last: char| last.is_ascii_digit()));
        if keep {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            push(&mut tokens, &current);
            current.clear();
        }
    }
    push(&mut tokens, &current);
    tokens
}

fn push(tokens: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim_matches('-');
    if !trimmed.is_empty() {
        tokens.push(trimmed.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_number, propose, FastPathContext, FastPathOp, NodeSummary, ScriptVar, TscnValueLite,
        DEFAULT_STEP_FACTOR, FAST_PATH_APPLY_BPS, FAST_PATH_CONFIRM_BPS,
    };

    fn var(name: &str, value: f64) -> ScriptVar {
        ScriptVar {
            name: name.to_owned(),
            value,
        }
    }

    fn ctx() -> FastPathContext {
        FastPathContext {
            presets_in_project: vec![
                "preset.rules.laps".to_owned(),
                "preset.ability.glide".to_owned(),
            ],
            nodes: vec![
                NodeSummary {
                    path: "/root/Game/Player".to_owned(),
                    class: "CharacterBody3D".to_owned(),
                    script_vars: vec![
                        var("jump_velocity", 5.5),
                        var("speed", 6.0),
                        var("glide_time", 3.0),
                    ],
                },
                NodeSummary {
                    path: "/root/Game/Enemies/Chaser".to_owned(),
                    class: "CharacterBody3D".to_owned(),
                    script_vars: vec![var("enemy_speed", 4.5), var("max_health", 30.0)],
                },
                NodeSummary {
                    path: "/root/Game/Enemies/Patroller".to_owned(),
                    class: "CharacterBody3D".to_owned(),
                    script_vars: vec![var("enemy_speed", 3.0)],
                },
                NodeSummary {
                    path: "/root/Game/Sun".to_owned(),
                    class: "DirectionalLight3D".to_owned(),
                    script_vars: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn the_golden_iteration_applies_without_asking() {
        let proposal = propose("make the glide 20% longer", &ctx()).expect("glide is a knob");
        assert_eq!(proposal.target.property, "glide_time");
        assert_eq!(
            proposal.target.node_path.as_deref(),
            Some("/root/Game/Player")
        );
        assert_eq!(proposal.op, FastPathOp::Multiply { factor: 1.2 });
        assert!(proposal.confidence_bps >= FAST_PATH_APPLY_BPS);
        assert!(proposal.applies_without_asking());
        assert_eq!(proposal.label, "Set glide_time 3 → 3.6");
    }

    #[test]
    fn a_bare_qualifier_still_applies_but_costs_a_little_confidence() {
        let proposal = propose("make the player jump higher", &ctx()).expect("jump is a knob");
        assert_eq!(proposal.target.property, "jump_velocity");
        assert_eq!(
            proposal.op,
            FastPathOp::Multiply {
                factor: DEFAULT_STEP_FACTOR
            }
        );
        assert!(proposal.confidence_bps < super::CONFIDENCE_UNIQUE_NODE_BPS);
        assert_eq!(proposal.label, "Set jump_velocity 5.5 → 6.6");
    }

    #[test]
    fn an_ambiguous_node_drops_to_confirm_and_lists_the_candidates() {
        let proposal = propose("make the enemies slower", &ctx()).expect("enemy speed is a knob");
        assert!(proposal.needs_choice());
        assert!(!proposal.applies_without_asking());
        assert_eq!(
            proposal.candidates,
            vec!["/root/Game/Enemies/Chaser", "/root/Game/Enemies/Patroller"]
        );
        assert!(proposal.confidence_bps >= FAST_PATH_CONFIRM_BPS);
        assert!(proposal.confidence_bps < FAST_PATH_APPLY_BPS);
    }

    #[test]
    fn a_class_property_finds_its_only_node() {
        let proposal = propose("make the sun brighter", &ctx()).expect("brightness is a knob");
        assert_eq!(proposal.target.property, "light_energy");
        assert_eq!(proposal.target.node_path.as_deref(), Some("/root/Game/Sun"));
        assert_eq!(proposal.label, "Multiply light_energy by 1.2");
    }

    #[test]
    fn a_preset_knob_with_no_node_still_resolves() {
        let proposal = propose("set the laps to 5", &ctx()).expect("laps is a knob");
        assert_eq!(
            proposal.target.preset_id.as_deref(),
            Some("preset.rules.laps")
        );
        assert_eq!(
            proposal.op,
            FastPathOp::Set {
                value: TscnValueLite::Number { value: 5.0 }
            }
        );
        assert_eq!(proposal.label, "Set lap_count 3 → 5");
    }

    #[test]
    fn half_double_and_add_all_read() {
        assert_eq!(
            propose("half the gravity", &ctx()).map(|p| p.op),
            None,
            "no node or preset owns gravity in this project"
        );
        let doubled = propose("double the player speed", &ctx()).expect("speed is a knob");
        assert_eq!(doubled.op, FastPathOp::Multiply { factor: 2.0 });
        let added = propose("add 2 lives", &ctx());
        assert!(added.is_none(), "lives is not in this project");
    }

    #[test]
    fn a_toggle_reads_on_and_off() {
        let context = FastPathContext {
            presets_in_project: vec!["preset.weather.rain".to_owned()],
            nodes: Vec::new(),
        };
        let proposal = propose("turn the rain off", &context).expect("rain toggles");
        assert_eq!(proposal.target.property, "rain_enabled");
        assert_eq!(
            proposal.op,
            FastPathOp::Set {
                value: TscnValueLite::Bool { value: false }
            }
        );
        assert_eq!(proposal.label, "Set rain_enabled to off");
    }

    #[test]
    fn a_choice_knob_sets_text() {
        let context = FastPathContext {
            presets_in_project: vec!["preset.rules.laps".to_owned()],
            nodes: Vec::new(),
        };
        let proposal =
            propose("set the difficulty to hard", &context).expect("difficulty is a knob");
        assert_eq!(
            proposal.op,
            FastPathOp::Set {
                value: TscnValueLite::Text {
                    value: "hard".to_owned()
                }
            }
        );
    }

    #[test]
    fn an_utterance_with_no_noun_or_no_qualifier_proposes_nothing() {
        assert!(propose("add a boss fight to level two", &ctx()).is_none());
        assert!(propose("the jump", &ctx()).is_none());
        assert!(propose("", &ctx()).is_none());
        assert!(propose("make it better", &ctx()).is_none());
    }

    /// Two lights in one scene: the class the noun table names, and the one the user means.
    fn two_lights() -> FastPathContext {
        FastPathContext {
            presets_in_project: Vec::new(),
            nodes: vec![
                NodeSummary {
                    path: "DirectionalLight3D".to_owned(),
                    class: "DirectionalLight3D".to_owned(),
                    script_vars: Vec::new(),
                },
                NodeSummary {
                    path: "Player/Lamp".to_owned(),
                    class: "OmniLight3D".to_owned(),
                    script_vars: Vec::new(),
                },
                NodeSummary {
                    path: "Floor".to_owned(),
                    class: "StaticBody3D".to_owned(),
                    script_vars: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn the_named_node_is_the_target_even_when_another_node_owns_the_class() {
        let proposal =
            propose("Set the Lamp's light_energy to 4", &two_lights()).expect("the Lamp is named");
        assert_eq!(proposal.target.property, "light_energy");
        assert_eq!(proposal.target.node_path.as_deref(), Some("Player/Lamp"));
        assert!(!proposal.needs_choice());
        assert!(proposal.applies_without_asking());
        assert!(
            proposal.rationale.contains("you named Player/Lamp"),
            "the rationale must say the node was named: {}",
            proposal.rationale
        );
        assert_eq!(
            proposal.op,
            FastPathOp::Set {
                value: TscnValueLite::Number { value: 4.0 }
            }
        );
    }

    #[test]
    fn a_named_node_that_cannot_carry_the_property_is_a_model_turn() {
        assert!(
            propose("Set the Floor's light_energy to 4", &two_lights()).is_none(),
            "a StaticBody3D has no light_energy, and the lights are not a fallback"
        );
    }

    #[test]
    fn two_requested_changes_are_never_half_applied() {
        assert!(
            propose(
                "Set the Lamp's light_energy to 4 and its light_color to a warm orange.",
                &two_lights(),
            )
            .is_none(),
            "half a request applied silently is worse than one model call"
        );
        assert!(propose("make the player jump higher and the enemies slower", &ctx()).is_none());
        assert!(propose("set the jump to 8; set the speed to 4", &ctx()).is_none());
    }

    #[test]
    fn one_clause_that_merely_reads_like_two_still_applies() {
        let proposal =
            propose("increase the enemy health", &ctx()).expect("one change, two noun words");
        assert_eq!(proposal.target.property, "max_health");
        assert_eq!(
            proposal.target.node_path.as_deref(),
            Some("/root/Game/Enemies/Chaser")
        );
    }

    #[test]
    fn two_lights_and_no_name_is_a_choice_not_a_guess() {
        let proposal = propose("make the light brighter", &two_lights()).expect("light is a knob");
        assert!(proposal.needs_choice());
        assert!(!proposal.applies_without_asking());
        assert_eq!(
            proposal.candidates,
            vec!["DirectionalLight3D", "Player/Lamp"]
        );
        assert!(proposal.confidence_bps >= FAST_PATH_CONFIRM_BPS);
    }

    #[test]
    fn one_light_and_no_name_still_applies() {
        let mut context = two_lights();
        context
            .nodes
            .retain(|node| node.path != "DirectionalLight3D");
        let proposal =
            propose("set the light_energy to 4", &context).expect("one light, one meaning");
        assert_eq!(proposal.target.node_path.as_deref(), Some("Player/Lamp"));
        assert!(proposal.applies_without_asking());
        assert_eq!(proposal.label, "Set light_energy to 4");
    }

    #[test]
    fn a_name_two_nodes_share_is_a_choice() {
        let context = FastPathContext {
            presets_in_project: Vec::new(),
            nodes: vec![
                NodeSummary {
                    path: "Player/Lamp".to_owned(),
                    class: "OmniLight3D".to_owned(),
                    script_vars: Vec::new(),
                },
                NodeSummary {
                    path: "Room/Lamp".to_owned(),
                    class: "SpotLight3D".to_owned(),
                    script_vars: Vec::new(),
                },
            ],
        };
        let proposal =
            propose("set the Lamp's light_energy to 4", &context).expect("both lamps could be it");
        assert!(proposal.needs_choice());
        assert_eq!(proposal.candidates, vec!["Player/Lamp", "Room/Lamp"]);
    }

    #[test]
    fn numbers_format_identically_everywhere() {
        assert_eq!(format_number(6.6000000000000005), "6.6");
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.8333333333), "0.83");
        assert_eq!(format_number(-0.001), "0");
    }
}
