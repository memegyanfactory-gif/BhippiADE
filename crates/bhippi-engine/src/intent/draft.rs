//! The deterministic first pass: a sentence in, a scored [`IntentDraft`] out, no model call.
//!
//! This is where the token thesis pays: everything a keyword can settle is settled here for
//! free, and the bounded model pass ([`crate::intent::delta`]) only ever sees what is left.
//! The pass is intentionally literal — it reports what the prompt *says*, marks what the
//! archetype *assumes*, and refuses to collapse a genuine conflict, which surfaces as
//! [`FactCertainty::Ambiguous`] with the competing readings attached.

use crate::game_spec::{FactCertainty, SpecFact};
use crate::intent::archetype::{self, Archetype, Dimension, Perspective};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;

/// Score a single-word genre keyword contributes.
pub const KEYWORD_WEIGHT_BPS: u32 = 1_000;
/// Score a multi-word genre phrase contributes. A phrase is far more specific than a word,
/// so "2d platformer" outranks a stray "platformer" without any tie-breaking heuristic.
pub const PHRASE_WEIGHT_BPS: u32 = 2_500;
/// Below this an archetype is a candidate but not a match, and the plan stays genre-less.
pub const MIN_ARCHETYPE_SCORE_BPS: u32 = 1_000;
/// At or above this the genre was literally typed, so the genre fact is `Certain`.
pub const CERTAIN_GENRE_SCORE_BPS: u32 = PHRASE_WEIGHT_BPS;
/// How many runner-up archetypes the plan card may show.
pub const MAX_ARCHETYPE_CANDIDATES: usize = 3;

/// Confidence for something the prompt states outright.
pub const CERTAIN_CONFIDENCE_BPS: u16 = 10_000;
/// Confidence for something only the archetype supplies.
pub const ASSUMED_CONFIDENCE_BPS: u16 = 6_000;
/// Confidence for a slot the prompt gives two readings of.
pub const AMBIGUOUS_CONFIDENCE_BPS: u16 = 4_000;

/// The largest spelled-out number the counter understands.
pub const MAX_NUMBER_WORD: u32 = 20;

/// Which part of the intent a fact settles.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum IntentSlot {
    Genre,
    Perspective,
    Dimension,
    ArtStyle,
    Setting,
    Win,
    Lose,
    Counts,
}

impl IntentSlot {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Genre => "genre",
            Self::Perspective => "perspective",
            Self::Dimension => "dimension",
            Self::ArtStyle => "art style",
            Self::Setting => "setting",
            Self::Win => "win condition",
            Self::Lose => "lose condition",
            Self::Counts => "counts",
        }
    }
}

/// A [`SpecFact`] with the slot it answers. `GameSpec` stores facts unkeyed because it
/// already knows which field each one sits in; a draft has not been placed yet, so the slot
/// travels with the fact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DraftFact {
    pub slot: IntentSlot,
    pub fact: SpecFact,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ArchetypeMatch {
    pub id: String,
    pub score_bps: u32,
    pub matched_keywords: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CountFact {
    pub noun: String,
    pub n: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct UnresolvedSlot {
    pub slot: IntentSlot,
    pub why: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct IntentDraft {
    pub archetype: Option<ArchetypeMatch>,
    pub candidates: Vec<ArchetypeMatch>,
    pub facts: Vec<DraftFact>,
    pub counts: Vec<CountFact>,
    pub unresolved: Vec<UnresolvedSlot>,
    pub normalized_prompt: String,
}

impl IntentDraft {
    /// The first fact for a slot, in the order the pass produced them.
    #[must_use]
    pub fn fact(&self, slot: IntentSlot) -> Option<&SpecFact> {
        self.facts
            .iter()
            .find(|entry| entry.slot == slot)
            .map(|entry| &entry.fact)
    }

    /// Every fact for a slot — `art_style` and `setting` routinely have several.
    #[must_use]
    pub fn facts_for(&self, slot: IntentSlot) -> Vec<&SpecFact> {
        self.facts
            .iter()
            .filter(|entry| entry.slot == slot)
            .map(|entry| &entry.fact)
            .collect()
    }

    /// Values the user stated outright. Only these may answer an archetype question: an
    /// assumption must never silently close a decision the plan card is meant to raise.
    #[must_use]
    pub fn certain_values(&self) -> BTreeSet<String> {
        self.facts
            .iter()
            .filter(|entry| entry.fact.certainty == FactCertainty::Certain)
            .map(|entry| entry.fact.value.clone())
            .collect()
    }

    /// The count the prompt gave for `noun`, if it gave one.
    #[must_use]
    pub fn count(&self, noun: &str) -> Option<u32> {
        self.counts
            .iter()
            .find(|count| count.noun == noun)
            .map(|count| count.n)
    }
}

/// Lowercase word tokens: letters, digits and inner hyphens survive, everything else is a
/// boundary. `"jump-and-glide,"` becomes `["jump-and-glide"]`; `"10"` stays a number.
#[must_use]
pub fn tokenize(prompt: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in prompt.chars() {
        if ch.is_alphanumeric() || ch == '-' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            push_token(&mut tokens, &current);
            current.clear();
        }
    }
    push_token(&mut tokens, &current);
    tokens
}

fn push_token(tokens: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim_matches('-');
    if !trimmed.is_empty() {
        tokens.push(trimmed.to_owned());
    }
}

/// The number `word` spells, for digits and for every English number word up to twenty.
#[must_use]
pub fn number_word(word: &str) -> Option<u32> {
    const WORDS: &[(&str, u32)] = &[
        ("zero", 0),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("thirteen", 13),
        ("fourteen", 14),
        ("fifteen", 15),
        ("sixteen", 16),
        ("seventeen", 17),
        ("eighteen", 18),
        ("nineteen", 19),
        ("twenty", 20),
    ];
    if word.chars().all(|ch| ch.is_ascii_digit()) && !word.is_empty() {
        return word.parse().ok();
    }
    WORDS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, value)| *value)
}

/// Compile one prompt into a draft. Deterministic and allocation-bounded: the same prompt
/// always produces the same draft, on every platform.
#[must_use]
pub fn draft(prompt: &str) -> IntentDraft {
    let tokens = tokenize(prompt);
    let normalized_prompt = tokens.join(" ");
    let padded = format!(" {} ", flatten(&normalized_prompt));

    let (matched, candidates) = match_archetypes(&padded);
    let pack = matched.as_ref().and_then(|hit| Archetype::find(&hit.id));

    let mut facts = Vec::new();
    let mut unresolved = Vec::new();

    if let (Some(hit), Some(pack)) = (matched.as_ref(), pack) {
        let certainty = if hit.score_bps >= CERTAIN_GENRE_SCORE_BPS {
            FactCertainty::Certain
        } else {
            FactCertainty::Assumed
        };
        facts.push(DraftFact {
            slot: IntentSlot::Genre,
            fact: SpecFact {
                value: pack.id.clone(),
                confidence_bps: if certainty == FactCertainty::Certain {
                    CERTAIN_CONFIDENCE_BPS
                } else {
                    ASSUMED_CONFIDENCE_BPS
                },
                certainty,
                alternatives: Vec::new(),
            },
        });
    } else {
        unresolved.push(UnresolvedSlot {
            slot: IntentSlot::Genre,
            why: "no archetype keyword matched, so the genre is still open".to_owned(),
        });
    }

    let perspective = read_perspective(&padded);
    resolve_enum_slot(
        IntentSlot::Perspective,
        &perspective,
        pack.map(|pack| perspective_value(pack.perspective)),
        &mut facts,
        &mut unresolved,
    );

    let stated_dimension = read_dimension(&padded);
    let implied = perspective
        .first()
        .filter(|_| perspective.len() == 1)
        .and_then(|value| perspective_from_value(value))
        .and_then(Perspective::implied_dimension)
        .map(dimension_value);
    resolve_enum_slot(
        IntentSlot::Dimension,
        &stated_dimension,
        implied.or_else(|| pack.map(|pack| dimension_value(pack.dimension))),
        &mut facts,
        &mut unresolved,
    );

    let art = matches_in_order(&padded, ART_VOCABULARY);
    if art.is_empty() {
        match pack.and_then(|pack| pack.art_vocabulary.first()) {
            Some(default) => facts.push(assumed(IntentSlot::ArtStyle, default)),
            None => unresolved.push(UnresolvedSlot {
                slot: IntentSlot::ArtStyle,
                why: "the prompt names no art direction and no archetype supplied one".to_owned(),
            }),
        }
    } else {
        for value in art {
            facts.push(certain(IntentSlot::ArtStyle, &value));
        }
    }

    let settings = matches_in_order(&padded, SETTING_NOUNS);
    if settings.is_empty() {
        unresolved.push(UnresolvedSlot {
            slot: IntentSlot::Setting,
            why: "the prompt names no place, so the level dressing is unchosen".to_owned(),
        });
    } else {
        for value in settings {
            facts.push(certain(IntentSlot::Setting, &value));
        }
    }

    let counts = read_counts(&tokens, &cue_phrases(&padded));

    let win = read_win(&padded, !counts.is_empty());
    resolve_enum_slot(
        IntentSlot::Win,
        &win,
        pack.map(|pack| win_for_rules(&pack.rules).to_owned()),
        &mut facts,
        &mut unresolved,
    );

    let lose = read_lose(&padded);
    resolve_enum_slot(IntentSlot::Lose, &lose, None, &mut facts, &mut unresolved);

    if !counts.is_empty() {
        let summary = counts
            .iter()
            .map(|count| format!("{}={}", count.noun, count.n))
            .collect::<Vec<_>>()
            .join(", ");
        facts.push(certain(IntentSlot::Counts, &summary));
    }

    IntentDraft {
        archetype: matched,
        candidates,
        facts,
        counts,
        unresolved,
        normalized_prompt,
    }
}

fn resolve_enum_slot(
    slot: IntentSlot,
    stated: &[String],
    fallback: Option<String>,
    facts: &mut Vec<DraftFact>,
    unresolved: &mut Vec<UnresolvedSlot>,
) {
    match stated.len() {
        0 => match fallback {
            Some(value) => facts.push(assumed(slot, &value)),
            None => unresolved.push(UnresolvedSlot {
                slot,
                why: format!(
                    "the prompt does not say and nothing implies the {}",
                    slot.label()
                ),
            }),
        },
        1 => facts.push(certain(slot, &stated[0])),
        _ => facts.push(DraftFact {
            slot,
            fact: SpecFact {
                value: stated[0].clone(),
                confidence_bps: AMBIGUOUS_CONFIDENCE_BPS,
                certainty: FactCertainty::Ambiguous,
                alternatives: stated.to_vec(),
            },
        }),
    }
}

fn certain(slot: IntentSlot, value: &str) -> DraftFact {
    DraftFact {
        slot,
        fact: SpecFact {
            value: value.to_owned(),
            confidence_bps: CERTAIN_CONFIDENCE_BPS,
            certainty: FactCertainty::Certain,
            alternatives: Vec::new(),
        },
    }
}

fn assumed(slot: IntentSlot, value: &str) -> DraftFact {
    DraftFact {
        slot,
        fact: SpecFact {
            value: value.to_owned(),
            confidence_bps: ASSUMED_CONFIDENCE_BPS,
            certainty: FactCertainty::Assumed,
            alternatives: Vec::new(),
        },
    }
}

fn match_archetypes(padded: &str) -> (Option<ArchetypeMatch>, Vec<ArchetypeMatch>) {
    let mut scored = Vec::new();
    for pack in archetype::builtin() {
        let mut score_bps = 0_u32;
        let mut matched_keywords = Vec::new();
        for keyword in &pack.keywords {
            if find_phrase(padded, keyword).is_some() {
                score_bps = score_bps.saturating_add(if is_phrase(keyword) {
                    PHRASE_WEIGHT_BPS
                } else {
                    KEYWORD_WEIGHT_BPS
                });
                matched_keywords.push(keyword.clone());
            }
        }
        if score_bps > 0 {
            scored.push(ArchetypeMatch {
                id: pack.id.clone(),
                score_bps,
                matched_keywords,
            });
        }
    }
    scored.sort_by(|left, right| {
        right
            .score_bps
            .cmp(&left.score_bps)
            .then_with(|| left.id.cmp(&right.id))
    });
    scored.truncate(MAX_ARCHETYPE_CANDIDATES);
    let matched = scored
        .first()
        .filter(|hit| hit.score_bps >= MIN_ARCHETYPE_SCORE_BPS)
        .cloned();
    (matched, scored)
}

fn read_perspective(padded: &str) -> Vec<String> {
    const CUES: &[(&str, &str)] = &[
        ("third person", "third_person"),
        ("3rd person", "third_person"),
        ("over the shoulder", "third_person"),
        ("first person", "first_person"),
        ("fps", "first_person"),
        ("top down", "top_down"),
        ("overhead", "top_down"),
        ("birds eye", "top_down"),
        ("side scrolling", "side_scroller"),
        ("side scroller", "side_scroller"),
        ("sidescroller", "side_scroller"),
        ("side on", "side_scroller"),
        ("isometric", "isometric"),
    ];
    matches_in_order(padded, CUES)
}

fn read_dimension(padded: &str) -> Vec<String> {
    const CUES: &[(&str, &str)] = &[
        ("3d", "three_d"),
        ("three dimensional", "three_d"),
        ("2d", "two_d"),
        ("two dimensional", "two_d"),
    ];
    matches_in_order(padded, CUES)
}

const WIN_CUES: &[(&str, &str)] = &[
    ("reach the", "reach-location"),
    ("reach a", "reach-location"),
    ("get to the", "reach-location"),
    ("make it to the", "reach-location"),
    ("escape", "escape"),
    ("survive", "survive-time"),
    ("defeat", "defeat-boss"),
    ("beat the boss", "defeat-boss"),
    ("kill the boss", "defeat-boss"),
    ("last one standing", "last-one-standing"),
    ("last player standing", "last-one-standing"),
    ("laps", "laps"),
    ("lap race", "laps"),
    ("finish first", "laps"),
    ("time trial", "time-trial"),
    ("solve every", "solve-all"),
    ("solve all", "solve-all"),
    ("defend the base", "defend-base"),
    ("defend your base", "defend-base"),
    ("protect the base", "defend-base"),
    ("as far as you can", "endless-distance"),
    ("as far as possible", "endless-distance"),
    ("high score", "endless-distance"),
    ("frag limit", "frag-limit"),
];

fn read_win(padded: &str, has_count: bool) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let collects = find_phrase(padded, "collect").is_some()
        || find_phrase(padded, "collecting").is_some()
        || find_phrase(padded, "gather").is_some();
    if collects && (has_count || find_phrase(padded, "unlock").is_some()) {
        found.push("collect-n".to_owned());
    }
    for value in matches_in_order(padded, WIN_CUES) {
        if !found.contains(&value) {
            found.push(value);
        }
    }
    found
}

const LOSE_CUES: &[(&str, &str)] = &[
    ("lives", "lives"),
    ("lose a life", "lives"),
    ("checkpoints", "checkpoints"),
    ("checkpoint system", "checkpoints"),
    ("one hit", "one-hit"),
    ("single hit", "one-hit"),
    ("time runs out", "timer"),
    ("before the timer", "timer"),
    ("countdown", "timer"),
];

fn read_lose(padded: &str) -> Vec<String> {
    matches_in_order(padded, LOSE_CUES)
}

/// The literal win and lose phrases this prompt contains, flattened. A number sitting inside
/// one of them is grammar rather than a quantity: "the last **one** standing" counts nothing,
/// and neither does "**one** hit and you are out".
fn cue_phrases(padded: &str) -> Vec<String> {
    WIN_CUES
        .iter()
        .chain(LOSE_CUES)
        .filter(|(phrase, _)| find_phrase(padded, phrase).is_some())
        .map(|(phrase, _)| flatten(phrase))
        .collect()
}

/// The win model an archetype's rules preset already commits to.
#[must_use]
pub fn win_for_rules(rules_preset: &str) -> &'static str {
    match rules_preset {
        "preset.rules.collect_n_to_unlock" => "collect-n",
        "preset.rules.reach_goal" => "reach-location",
        "preset.rules.laps" => "laps",
        "preset.rules.survive_time" => "survive-time",
        "preset.rules.last_one_standing" => "last-one-standing",
        "preset.rules.frag_limit" => "frag-limit",
        "preset.rules.defend_base" => "defend-base",
        "preset.rules.solve_puzzle" => "solve-all",
        _ => "endless-distance",
    }
}

fn read_counts(tokens: &[String], cues: &[String]) -> Vec<CountFact> {
    const FUNCTION_WORDS: &[&str] = &[
        "of", "to", "and", "or", "the", "a", "an", "in", "on", "for", "with", "per", "by", "at",
    ];
    let mut counts = Vec::new();
    for pair in tokens.windows(2) {
        let (Some(n), noun) = (number_word(&pair[0]), pair[1].as_str()) else {
            continue;
        };
        let literal = flatten(&format!("{} {noun}", pair[0]));
        if cues.iter().any(|cue| cue.contains(&literal)) {
            continue;
        }
        if FUNCTION_WORDS.contains(&noun)
            || noun.chars().next().is_some_and(|ch| ch.is_ascii_digit())
            || counts.iter().any(|count: &CountFact| count.noun == noun)
        {
            continue;
        }
        counts.push(CountFact {
            noun: noun.to_owned(),
            n,
        });
    }
    counts
}

const ART_VOCABULARY: &[(&str, &str)] = &[
    ("low-poly", "low-poly"),
    ("lowpoly", "low-poly"),
    ("pixel-art", "pixel"),
    ("pixel", "pixel"),
    ("voxel", "voxel"),
    ("cel-shaded", "cel-shaded"),
    ("flat-shaded", "flat-shaded"),
    ("hand-painted", "hand-painted"),
    ("painterly", "painterly"),
    ("watercolour", "painterly"),
    ("watercolor", "painterly"),
    ("realistic", "realistic"),
    ("photoreal", "realistic"),
    ("cozy", "cozy"),
    ("cosy", "cozy"),
    ("dark", "dark"),
    ("moody", "moody"),
    ("neon", "neon"),
    ("synthwave", "synthwave"),
    ("cartoon", "cartoon"),
    ("cartoony", "cartoon"),
    ("retro", "retro"),
    ("minimalist", "minimalist"),
    ("gritty", "gritty"),
    ("pastel", "pastel"),
    ("noir", "noir"),
    ("stylised", "stylised"),
    ("stylized", "stylised"),
    ("monochrome", "monochrome"),
    ("muted", "muted"),
    ("bright", "bright"),
    ("toy-like", "toy-like"),
    ("industrial", "industrial"),
    ("chrome", "chrome"),
    ("sun-bleached", "sun-bleached"),
    ("clean", "clean"),
    ("white", "white"),
    ("warm", "warm"),
];

/// The canonical art value a single word carries, if the pass recognises it at all.
///
/// Exposed so the archetype packs and this table can be held to the same vocabulary: a pack
/// that names an art word the draft cannot read would quietly turn a stated style into an
/// assumed one.
#[must_use]
pub fn art_value(word: &str) -> Option<String> {
    matches_in_order(&format!(" {} ", flatten(word)), ART_VOCABULARY)
        .into_iter()
        .next()
}

const SETTING_NOUNS: &[(&str, &str)] = &[
    ("islands", "island"),
    ("island", "island"),
    ("archipelago", "island"),
    ("lighthouse", "lighthouse"),
    ("dungeons", "dungeon"),
    ("dungeon", "dungeon"),
    ("city", "city"),
    ("cyberpunk city", "city"),
    ("rooftops", "rooftop"),
    ("rooftop", "rooftop"),
    ("space station", "station"),
    ("station", "station"),
    ("spaceship", "spaceship"),
    ("space", "space"),
    ("moon", "moon"),
    ("forest", "forest"),
    ("jungle", "jungle"),
    ("desert", "desert"),
    ("arena", "arena"),
    ("racetrack", "track"),
    ("race track", "track"),
    ("track", "track"),
    ("mountains", "mountain"),
    ("mountain", "mountain"),
    ("underwater", "ocean"),
    ("ocean", "ocean"),
    ("caves", "cave"),
    ("cave", "cave"),
    ("castle", "castle"),
    ("ruins", "ruins"),
    ("temple", "temple"),
    ("factory", "factory"),
    ("warehouse", "factory"),
    ("highway", "highway"),
    ("swamp", "swamp"),
    ("canyon", "canyon"),
    ("village", "village"),
    ("farm", "farm"),
    ("laboratory", "lab"),
    ("volcano", "volcano"),
    ("junkyard", "junkyard"),
    ("snow", "snow"),
    ("tundra", "snow"),
    ("valley", "valley"),
    ("garden", "garden"),
    ("sewer", "sewer"),
    ("museum", "museum"),
];

/// Canonical values matched in the order they appear in the prompt, longest phrase first so
/// `pixel-art` never also reports a bare `pixel`.
fn matches_in_order(padded: &str, table: &[(&str, &str)]) -> Vec<String> {
    let mut hits: Vec<(usize, usize, String)> = Vec::new();
    for (phrase, value) in table {
        let Some(at) = find_phrase(padded, phrase) else {
            continue;
        };
        let width = flatten(phrase).len();
        if hits.iter().any(|(other_at, other_width, _)| {
            *other_at <= at && at + width <= other_at + other_width
        }) {
            continue;
        }
        hits.retain(|(other_at, other_width, _)| {
            !(at <= *other_at && other_at + other_width <= at + width)
        });
        hits.push((at, width, (*value).to_owned()));
    }
    hits.sort_by_key(|left| left.0);
    let mut values = Vec::new();
    for (_, _, value) in hits {
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

fn is_phrase(keyword: &str) -> bool {
    keyword.contains(' ') || keyword.contains('-')
}

/// Byte offset of `phrase` inside an already-padded, already-flattened haystack, matched on
/// whole-word boundaries so `lap` never fires inside `collapse`.
pub(crate) fn find_phrase(padded: &str, phrase: &str) -> Option<usize> {
    let needle = format!(" {} ", flatten(phrase));
    padded.find(&needle).map(|at| at + 1)
}

/// Lowercase, hyphens become spaces, runs of whitespace collapse. `"Low-Poly  Islands"` and
/// `"low poly islands"` flatten to the same text, so the tables need one spelling each.
pub(crate) fn flatten(text: &str) -> String {
    text.to_lowercase()
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn perspective_value(perspective: Perspective) -> String {
    match perspective {
        Perspective::ThirdPerson => "third_person",
        Perspective::FirstPerson => "first_person",
        Perspective::TopDown => "top_down",
        Perspective::SideScroller => "side_scroller",
        Perspective::Isometric => "isometric",
    }
    .to_owned()
}

fn perspective_from_value(value: &str) -> Option<Perspective> {
    match value {
        "third_person" => Some(Perspective::ThirdPerson),
        "first_person" => Some(Perspective::FirstPerson),
        "top_down" => Some(Perspective::TopDown),
        "side_scroller" => Some(Perspective::SideScroller),
        "isometric" => Some(Perspective::Isometric),
        _ => None,
    }
}

fn dimension_value(dimension: Dimension) -> String {
    match dimension {
        Dimension::TwoD => "two_d",
        Dimension::ThreeD => "three_d",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        draft, number_word, tokenize, win_for_rules, IntentSlot, ASSUMED_CONFIDENCE_BPS,
        CERTAIN_CONFIDENCE_BPS, MAX_ARCHETYPE_CANDIDATES, MAX_NUMBER_WORD,
    };
    use crate::game_spec::FactCertainty;

    #[test]
    fn the_tokeniser_keeps_numbers_and_hyphenated_words_and_drops_punctuation() {
        assert_eq!(
            tokenize("A cozy, third-person game — collect 10 feathers!"),
            vec![
                "a",
                "cozy",
                "third-person",
                "game",
                "collect",
                "10",
                "feathers"
            ]
        );
        assert_eq!(tokenize("   "), Vec::<String>::new());
        assert_eq!(tokenize("--x--"), vec!["x"]);
    }

    #[test]
    fn number_words_read_up_to_twenty_and_digits_without_a_ceiling() {
        assert_eq!(number_word("three"), Some(3));
        assert_eq!(number_word("twenty"), Some(MAX_NUMBER_WORD));
        assert_eq!(number_word("thirty"), None);
        assert_eq!(number_word("10"), Some(10));
        assert_eq!(number_word("250"), Some(250));
        assert_eq!(number_word("feathers"), None);
    }

    #[test]
    fn an_explicit_word_is_certain_and_an_archetype_default_is_assumed() {
        let drafted = draft("a first person arena shooter in a neon city");
        let perspective = drafted
            .fact(IntentSlot::Perspective)
            .expect("perspective read");
        assert_eq!(perspective.value, "first_person");
        assert_eq!(perspective.certainty, FactCertainty::Certain);
        assert_eq!(perspective.confidence_bps, CERTAIN_CONFIDENCE_BPS);

        let dimension = drafted.fact(IntentSlot::Dimension).expect("dimension read");
        assert_eq!(dimension.value, "three_d");
        assert_eq!(dimension.certainty, FactCertainty::Assumed);
        assert_eq!(dimension.confidence_bps, ASSUMED_CONFIDENCE_BPS);
    }

    #[test]
    fn conflicting_cues_stay_ambiguous_with_both_readings_recorded() {
        let drafted = draft("a 2d game rendered with 3d models, side scrolling");
        let dimension = drafted.fact(IntentSlot::Dimension).expect("dimension read");
        assert_eq!(dimension.certainty, FactCertainty::Ambiguous);
        assert_eq!(dimension.alternatives.len(), 2);
        assert!(dimension.alternatives.contains(&"two_d".to_owned()));
        assert!(dimension.alternatives.contains(&"three_d".to_owned()));
    }

    #[test]
    fn the_draft_reads_every_art_word_the_packs_themselves_name() {
        for pack in crate::intent::archetype::builtin() {
            for word in &pack.art_vocabulary {
                assert!(
                    super::art_value(word).is_some(),
                    "{} names art word {word:?} the draft cannot read",
                    pack.id
                );
            }
        }
    }

    #[test]
    fn a_longer_art_phrase_suppresses_the_word_inside_it() {
        let styles = draft("a pixel-art dungeon crawler")
            .facts_for(IntentSlot::ArtStyle)
            .into_iter()
            .map(|fact| fact.value.clone())
            .collect::<Vec<_>>();
        assert_eq!(styles, vec!["pixel"]);
    }

    #[test]
    fn counts_read_digits_and_number_words_and_skip_function_words() {
        let drafted = draft("collect 10 feathers across three levels in a forest");
        assert_eq!(drafted.count("feathers"), Some(10));
        assert_eq!(drafted.count("levels"), Some(3));
        assert_eq!(drafted.counts.len(), 2);
    }

    #[test]
    fn a_number_inside_a_win_or_lose_phrase_is_grammar_rather_than_a_count() {
        let pronoun = draft("a deathmatch where the last one standing wins");
        assert_eq!(pronoun.counts, Vec::new());
        assert_eq!(
            pronoun
                .fact(IntentSlot::Win)
                .map(|fact| fact.value.as_str()),
            Some("last-one-standing")
        );

        let article = draft("an endless runner where one hit ends the run");
        assert_eq!(article.counts, Vec::new());
        assert_eq!(
            article
                .fact(IntentSlot::Lose)
                .map(|fact| fact.value.as_str()),
            Some("one-hit")
        );

        // The guard is narrow: a real quantity beside a cue word still counts.
        let real = draft("survive with three lives");
        assert_eq!(real.count("lives"), Some(3));
    }

    #[test]
    fn candidates_are_capped_and_ordered_by_score_then_id() {
        let drafted = draft("a 2d platformer, a side scroller, run and jump");
        assert!(drafted.candidates.len() <= MAX_ARCHETYPE_CANDIDATES);
        assert_eq!(
            drafted.archetype.as_ref().map(|hit| hit.id.as_str()),
            Some("platformer_2d")
        );
        let scores = drafted
            .candidates
            .iter()
            .map(|hit| hit.score_bps)
            .collect::<Vec<_>>();
        assert!(scores.windows(2).all(|pair| pair[0] >= pair[1]));
    }

    #[test]
    fn an_off_archetype_prompt_matches_nothing_and_says_what_is_missing() {
        let drafted = draft("a spreadsheet for tracking rent payments");
        assert!(drafted.archetype.is_none());
        assert!(drafted.candidates.is_empty());
        assert!(drafted
            .unresolved
            .iter()
            .any(|slot| slot.slot == IntentSlot::Genre));
    }

    #[test]
    fn the_golden_prompt_reads_the_way_the_plan_says_it_should() {
        let drafted = draft(
            "a cozy third-person exploration game with jump-and-glide, low-poly islands, \
             collect 10 feathers to unlock the lighthouse",
        );
        assert_eq!(
            drafted.archetype.as_ref().map(|hit| hit.id.as_str()),
            Some("exploration")
        );
        assert_eq!(
            drafted
                .fact(IntentSlot::Perspective)
                .map(|f| f.value.as_str()),
            Some("third_person")
        );
        assert_eq!(
            drafted
                .fact(IntentSlot::Dimension)
                .map(|f| f.value.as_str()),
            Some("three_d")
        );
        let styles = drafted
            .facts_for(IntentSlot::ArtStyle)
            .into_iter()
            .map(|fact| fact.value.clone())
            .collect::<Vec<_>>();
        assert!(styles.contains(&"cozy".to_owned()));
        assert!(styles.contains(&"low-poly".to_owned()));
        assert_eq!(drafted.count("feathers"), Some(10));
        let win = drafted.fact(IntentSlot::Win).expect("win read");
        assert_eq!(win.value, "collect-n");
        assert_eq!(win.certainty, FactCertainty::Certain);
    }

    #[test]
    fn a_rules_preset_implies_its_win_model() {
        assert_eq!(win_for_rules("preset.rules.laps"), "laps");
        assert_eq!(
            win_for_rules("preset.rules.collect_n_to_unlock"),
            "collect-n"
        );
        assert_eq!(
            win_for_rules("preset.rules.endless_distance"),
            "endless-distance"
        );
    }

    #[test]
    fn drafting_is_deterministic_for_the_same_prompt() {
        let prompt = "an isometric tower defense with fixed path lanes and twenty waves";
        assert_eq!(draft(prompt), draft(prompt));
    }
}
