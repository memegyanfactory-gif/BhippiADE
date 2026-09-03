//! The design knowledge base, its budgeted retrieval, and the taste loop's memories
//! (ADR-0046, `docs/18-DESIGN-INTELLIGENCE-ARCHITECTURE.md`).
//!
//! Three things live here and nowhere else:
//!
//! * [`DesignKb`] — the versioned Markdown modules under `prompts/design/`, compiled in
//!   with `include_str!`, parsed into sections, indexed, searched, and **selected under a
//!   budget** per turn. The base is data; this module only ever reads it (INV-091).
//! * [`TasteProfile`] — the semantic memory: pinned preferences with an origin and a
//!   weight, merged from typed [`TasteSignal`]s by deterministic rules (INV-094).
//! * [`LessonBook`] — the procedural memory: rules distilled from episodes, **proposed** by
//!   Rust or the model and **approved** only by the user (INV-094).
//!
//! Nothing here depends on `bhippi-engine` (ADR-0040): the engine-facing half — scoring a
//! model against Godot resources, the contrast gate — lives in the engine crate and is
//! assembled with this at the `bhippi-app` edge.

use crate::context::estimate_text_tokens;
use bhippi_types::{
    DesignSurface, DESIGN_CONTEXT_TOKEN_BUDGET, DESIGN_LESSONS_MAX_APPROVED,
    DESIGN_LESSON_MAX_RULE_BYTES, DESIGN_LESSON_MIN_EVIDENCE, DESIGN_MAX_SECTIONS_PER_TURN,
    DESIGN_QUERY_ANSWER_TOKEN_BUDGET, DESIGN_SEARCH_MAX_HITS, TASTE_PROFILE_MAX_PINS,
    TASTE_PROFILE_TOKEN_BUDGET,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The knowledge-base file format. A module whose `version:` major is newer is refused.
pub const DESIGN_KB_FORMAT: &str = "bhippi-design-kb@1";
/// The major this parser understands.
pub const DESIGN_KB_MAJOR: u32 = 1;
/// The taste profile's document format.
pub const TASTE_FORMAT: &str = "bhippi-taste@1";
/// The lesson book's document format.
pub const LESSON_FORMAT: &str = "bhippi-design-lesson@1";
/// The episode projection's document format.
pub const DESIGN_EPISODE_FORMAT: &str = "bhippi-design-episode@1";

/// Longest text an episode or a taste value may carry: a projection, never a transcript.
const MAX_PROJECTION_BYTES: usize = 400;

const INDEX_SOURCE: &str = include_str!("../../../prompts/design/INDEX.md");

/// Every module, compiled in. The id is the path under `prompts/design/` without `.md`;
/// the integration test walks the directory so a file that is not listed here fails CI.
const MODULE_SOURCES: &[(&str, &str)] = &[
    (
        "foundations/judgements",
        include_str!("../../../prompts/design/foundations/judgements.md"),
    ),
    (
        "foundations/color",
        include_str!("../../../prompts/design/foundations/color.md"),
    ),
    (
        "foundations/type",
        include_str!("../../../prompts/design/foundations/type.md"),
    ),
    (
        "foundations/space-layout",
        include_str!("../../../prompts/design/foundations/space-layout.md"),
    ),
    (
        "foundations/shape-elevation",
        include_str!("../../../prompts/design/foundations/shape-elevation.md"),
    ),
    (
        "foundations/motion",
        include_str!("../../../prompts/design/foundations/motion.md"),
    ),
    (
        "foundations/copy",
        include_str!("../../../prompts/design/foundations/copy.md"),
    ),
    (
        "foundations/states-a11y",
        include_str!("../../../prompts/design/foundations/states-a11y.md"),
    ),
    (
        "foundations/anti-slop",
        include_str!("../../../prompts/design/foundations/anti-slop.md"),
    ),
    (
        "foundations/icons-imagery",
        include_str!("../../../prompts/design/foundations/icons-imagery.md"),
    ),
    (
        "process/design-plan",
        include_str!("../../../prompts/design/process/design-plan.md"),
    ),
    (
        "process/critique",
        include_str!("../../../prompts/design/process/critique.md"),
    ),
    (
        "process/handoff",
        include_str!("../../../prompts/design/process/handoff.md"),
    ),
    (
        "web/page-anatomy",
        include_str!("../../../prompts/design/web/page-anatomy.md"),
    ),
    (
        "web/fonts",
        include_str!("../../../prompts/design/web/fonts.md"),
    ),
    (
        "web/themes-responsive",
        include_str!("../../../prompts/design/web/themes-responsive.md"),
    ),
    (
        "web/dynamic",
        include_str!("../../../prompts/design/web/dynamic.md"),
    ),
    (
        "web/charts",
        include_str!("../../../prompts/design/web/charts.md"),
    ),
    (
        "game-ui/hud",
        include_str!("../../../prompts/design/game-ui/hud.md"),
    ),
    (
        "game-ui/menus-flow",
        include_str!("../../../prompts/design/game-ui/menus-flow.md"),
    ),
    (
        "game-ui/godot-control",
        include_str!("../../../prompts/design/game-ui/godot-control.md"),
    ),
    (
        "game-ui/feedback-juice",
        include_str!("../../../prompts/design/game-ui/feedback-juice.md"),
    ),
    (
        "scene-3d/composition",
        include_str!("../../../prompts/design/scene-3d/composition.md"),
    ),
    (
        "scene-3d/layout-metrics",
        include_str!("../../../prompts/design/scene-3d/layout-metrics.md"),
    ),
    (
        "scene-3d/level-flow",
        include_str!("../../../prompts/design/scene-3d/level-flow.md"),
    ),
    (
        "scene-3d/lighting-environment",
        include_str!("../../../prompts/design/scene-3d/lighting-environment.md"),
    ),
    (
        "scene-3d/materials-palette",
        include_str!("../../../prompts/design/scene-3d/materials-palette.md"),
    ),
    (
        "scene-3d/camera",
        include_str!("../../../prompts/design/scene-3d/camera.md"),
    ),
    (
        "scene-3d/model-selection",
        include_str!("../../../prompts/design/scene-3d/model-selection.md"),
    ),
    (
        "scene-2d/sprites-tiles",
        include_str!("../../../prompts/design/scene-2d/sprites-tiles.md"),
    ),
    (
        "art-direction/brief",
        include_str!("../../../prompts/design/art-direction/brief.md"),
    ),
    (
        "art-direction/styles",
        include_str!("../../../prompts/design/art-direction/styles.md"),
    ),
    (
        "audio/sound-design",
        include_str!("../../../prompts/design/audio/sound-design.md"),
    ),
    (
        "learning/taste-loop",
        include_str!("../../../prompts/design/learning/taste-loop.md"),
    ),
];

// ─────────────────────────────────────────────────────────────────────────────────────────
// The knowledge base
// ─────────────────────────────────────────────────────────────────────────────────────────

/// The nine domains a module may belong to. The id's first path segment must agree.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesignDomain {
    Foundations,
    Process,
    Web,
    GameUi,
    Scene3d,
    Scene2d,
    ArtDirection,
    Audio,
    Learning,
}

impl DesignDomain {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Foundations => "foundations",
            Self::Process => "process",
            Self::Web => "web",
            Self::GameUi => "game-ui",
            Self::Scene3d => "scene-3d",
            Self::Scene2d => "scene-2d",
            Self::ArtDirection => "art-direction",
            Self::Audio => "audio",
            Self::Learning => "learning",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "foundations" => Some(Self::Foundations),
            "process" => Some(Self::Process),
            "web" => Some(Self::Web),
            "game-ui" => Some(Self::GameUi),
            "scene-3d" => Some(Self::Scene3d),
            "scene-2d" => Some(Self::Scene2d),
            "art-direction" => Some(Self::ArtDirection),
            "audio" => Some(Self::Audio),
            "learning" => Some(Self::Learning),
            _ => None,
        }
    }
}

/// One addressable section of a module: `module#section`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignSection {
    /// `module#section`, the id the model asks for.
    pub id: String,
    /// The section's own id within its module.
    pub section: String,
    pub title: String,
    pub body: String,
    /// Estimated tokens of the rendered section (heading plus body).
    pub tokens: u64,
}

impl DesignSection {
    fn render(&self) -> String {
        format!("## {}\n{}\n", self.title, self.body)
    }
}

/// One parsed module of the base.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignModule {
    pub id: String,
    pub version: u32,
    pub domain: DesignDomain,
    pub title: String,
    /// One line: when the model should read this module.
    pub when: String,
    pub tags: Vec<String>,
    /// Prose before the first section marker, if any.
    pub intro: String,
    pub sections: Vec<DesignSection>,
    /// Estimated tokens of the whole module.
    pub tokens: u64,
}

/// Why a module or the index could not be read. Every variant names the file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DesignError {
    UnsupportedMajor { module: String, version: u32 },
    MalformedHeader { module: String, line: String },
    MissingHeaderKey { module: String, key: &'static str },
    UnknownDomain { module: String, domain: String },
    DomainMismatch { module: String, header: String },
    DuplicateTag { module: String, tag: String },
    BadSectionId { module: String, section: String },
    DuplicateSection { module: String, section: String },
    MissingHeading { module: String, section: String },
    NoSections { module: String },
    IndexMissing { module: String },
    IndexOrphan { module: String },
    IndexDuplicate { module: String },
}

impl fmt::Display for DesignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedMajor { module, version } => write!(
                f,
                "design module `{module}` declares version {version}; this build reads major {DESIGN_KB_MAJOR}"
            ),
            Self::MalformedHeader { module, line } => {
                write!(f, "design module `{module}`: header line is not `key: value`: {line:?}")
            }
            Self::MissingHeaderKey { module, key } => {
                write!(f, "design module `{module}`: header lacks `{key}:`")
            }
            Self::UnknownDomain { module, domain } => {
                write!(f, "design module `{module}`: unknown domain `{domain}`")
            }
            Self::DomainMismatch { module, header } => write!(
                f,
                "design module `{module}`: header domain `{header}` disagrees with the id's path"
            ),
            Self::DuplicateTag { module, tag } => {
                write!(f, "design module `{module}`: tag `{tag}` listed twice")
            }
            Self::BadSectionId { module, section } => write!(
                f,
                "design module `{module}`: section id `{section}` must be lowercase letters, digits and hyphens"
            ),
            Self::DuplicateSection { module, section } => {
                write!(f, "design module `{module}`: section `{section}` marked twice")
            }
            Self::MissingHeading { module, section } => write!(
                f,
                "design module `{module}`: section `{section}` is not followed by a heading"
            ),
            Self::NoSections { module } => {
                write!(f, "design module `{module}` has no `<!-- section: … -->` marker")
            }
            Self::IndexMissing { module } => {
                write!(f, "prompts/design/INDEX.md lists `{module}` but no such module is bundled")
            }
            Self::IndexOrphan { module } => {
                write!(f, "design module `{module}` is bundled but prompts/design/INDEX.md does not list it")
            }
            Self::IndexDuplicate { module } => {
                write!(f, "prompts/design/INDEX.md lists `{module}` twice")
            }
        }
    }
}

impl std::error::Error for DesignError {}

/// A ranked search hit: the id to ask for, and enough to decide whether to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub when: String,
    pub score: u32,
}

/// A search over the base.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SearchQuery {
    pub text: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

const fn default_search_limit() -> usize {
    DESIGN_SEARCH_MAX_HITS
}

impl SearchQuery {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            domain: None,
            limit: DESIGN_SEARCH_MAX_HITS,
        }
    }
}

/// What a turn is about, so Rust can select the pack (docs/18 §3.2).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignRequest {
    pub surface: DesignSurface,
    /// From the archetype, the message and the batch verbs. Lowercased on use.
    #[serde(default)]
    pub intent_tags: Vec<String>,
    /// The brief's style pack id, if a brief exists.
    #[serde(default)]
    pub style_pack: Option<String>,
    pub budget_tokens: u64,
    pub max_sections: usize,
    /// Sections the caller insists on, first, in this order.
    #[serde(default)]
    pub pinned: Vec<String>,
}

impl DesignRequest {
    #[must_use]
    pub fn new(surface: DesignSurface) -> Self {
        Self {
            surface,
            intent_tags: Vec::new(),
            style_pack: None,
            budget_tokens: DESIGN_CONTEXT_TOKEN_BUDGET,
            max_sections: DESIGN_MAX_SECTIONS_PER_TURN,
            pinned: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.intent_tags
            .extend(tags.into_iter().map(|tag| tag.into().to_lowercase()));
        self
    }

    #[must_use]
    pub fn with_pinned<I, S>(mut self, pinned: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.pinned.extend(pinned.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn with_style_pack(mut self, pack: impl Into<String>) -> Self {
        self.style_pack = Some(pack.into().to_lowercase());
        self
    }
}

/// One section in a pack, as the token report sees it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackedSection {
    pub id: String,
    pub title: String,
    pub tokens: u64,
    pub pinned: bool,
}

/// The pack Rust selected for a turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignPack {
    pub sections: Vec<PackedSection>,
    pub tokens: u64,
    /// Scored sections that did not fit, so the model knows what to ask for.
    pub left_out: Vec<String>,
    text: String,
}

impl DesignPack {
    /// The block appended to the system prompt.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Why a pack could not be assembled. A pinned overflow **blocks**; it never truncates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DesignSelectError {
    UnknownPin {
        id: String,
    },
    PinnedOverflow {
        id: String,
        tokens: u64,
        budget: u64,
    },
}

impl fmt::Display for DesignSelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPin { id } => write!(f, "pinned design section `{id}` does not exist"),
            Self::PinnedOverflow { id, tokens, budget } => write!(
                f,
                "pinned design sections exceed the budget at `{id}` ({tokens} of {budget} tokens); raise the budget or pin less"
            ),
        }
    }
}

impl std::error::Error for DesignSelectError {}

/// A mid-turn `<design_query>` (docs/18 §3.3).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesignQuery {
    Section {
        id: String,
    },
    Search {
        q: String,
        #[serde(default)]
        domain: Option<String>,
    },
    Style {
        id: String,
    },
    Fonts {
        #[serde(default)]
        mood: Option<String>,
        #[serde(default)]
        surface: Option<String>,
    },
    Taste,
}

/// A capped answer to a [`DesignQuery`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignAnswer {
    pub text: String,
    pub capped: bool,
}

/// The parsed, indexed base.
#[derive(Clone, Debug)]
pub struct DesignKb {
    modules: Vec<DesignModule>,
}

impl DesignKb {
    /// The base compiled into this binary. Fails only if a module or the index is malformed,
    /// which the tests catch before a release does.
    pub fn bundled() -> Result<Self, DesignError> {
        Self::parse(INDEX_SOURCE, MODULE_SOURCES)
    }

    /// Parse an index and a set of `(id, source)` modules. Every index entry must resolve
    /// and every module must be indexed; the result is ordered as the index lists them.
    pub fn parse(index: &str, sources: &[(&str, &str)]) -> Result<Self, DesignError> {
        let order = parse_index(index)?;
        let mut by_id: BTreeMap<String, DesignModule> = BTreeMap::new();
        for (id, source) in sources {
            let module = parse_module(id, source)?;
            by_id.insert((*id).to_owned(), module);
        }
        for id in by_id.keys() {
            if !order.iter().any(|listed| listed == id) {
                return Err(DesignError::IndexOrphan { module: id.clone() });
            }
        }
        let mut modules = Vec::with_capacity(order.len());
        for id in &order {
            let module = by_id
                .remove(id)
                .ok_or_else(|| DesignError::IndexMissing { module: id.clone() })?;
            modules.push(module);
        }
        Ok(Self { modules })
    }

    /// Every module, in index order.
    #[must_use]
    pub fn modules(&self) -> &[DesignModule] {
        &self.modules
    }

    #[must_use]
    pub fn module(&self, id: &str) -> Option<&DesignModule> {
        self.modules.iter().find(|module| module.id == id)
    }

    /// Look a section up by `module#section`.
    #[must_use]
    pub fn section(&self, id: &str) -> Option<&DesignSection> {
        let (module_id, section_id) = id.split_once('#')?;
        self.module(module_id)?
            .sections
            .iter()
            .find(|section| section.section == section_id)
    }

    /// The always-on map: one line per module, grouped by domain, in index order.
    #[must_use]
    pub fn render_index(&self) -> String {
        let mut out = String::from("## Design base — ask with design_query\n");
        let mut current: Option<DesignDomain> = None;
        for module in &self.modules {
            if current != Some(module.domain) {
                out.push_str(&format!("### {}\n", module.domain.as_str()));
                current = Some(module.domain);
            }
            out.push_str(&format!("- `{}` — {}\n", module.id, module.when));
        }
        out
    }

    /// Estimated tokens of [`Self::render_index`].
    #[must_use]
    pub fn index_tokens(&self) -> u64 {
        estimate_text_tokens(&self.render_index())
    }

    /// Ranked hits for a query. Deterministic: ties break on index order.
    #[must_use]
    pub fn search(&self, query: &SearchQuery) -> Vec<SearchHit> {
        let words = query_words(&query.text);
        if words.is_empty() {
            return Vec::new();
        }
        let domain = query.domain.as_deref().and_then(DesignDomain::parse);
        if query.domain.is_some() && domain.is_none() {
            return Vec::new();
        }
        let mut scored: Vec<(u32, usize, usize)> = Vec::new();
        for (module_idx, module) in self.modules.iter().enumerate() {
            if let Some(domain) = domain {
                if module.domain != domain {
                    continue;
                }
            }
            let module_title_words = split_words(&module.title);
            for (section_idx, section) in module.sections.iter().enumerate() {
                let section_title_words = split_words(&section.title);
                let section_id_words: Vec<String> =
                    section.section.split('-').map(str::to_owned).collect();
                let body = section.body.to_lowercase();
                let mut score = 0_u32;
                let mut body_hits = 0_u32;
                for word in &words {
                    if module.tags.iter().any(|tag| term_matches(word, tag)) {
                        score += 3;
                    }
                    if section_title_words.iter().any(|t| term_matches(word, t)) {
                        score += 2;
                    }
                    if section_id_words.iter().any(|t| term_matches(word, t)) {
                        score += 2;
                    }
                    if module_title_words.iter().any(|t| term_matches(word, t)) {
                        score += 1;
                    }
                    if body_hits < 3 && body.contains(word.as_str()) {
                        body_hits += 1;
                    }
                }
                score += body_hits;
                if score > 0 {
                    scored.push((score, module_idx, section_idx));
                }
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        scored
            .into_iter()
            .take(query.limit.clamp(1, DESIGN_SEARCH_MAX_HITS))
            .map(|(score, module_idx, section_idx)| {
                let module = &self.modules[module_idx];
                let section = &module.sections[section_idx];
                SearchHit {
                    id: section.id.clone(),
                    title: format!("{} › {}", module.title, section.title),
                    when: module.when.clone(),
                    score,
                }
            })
            .collect()
    }

    /// Select the pack for a turn (docs/18 §3.2): pinned sections first, then ranked
    /// sections while the budget and the section cap hold. A pinned overflow is an error,
    /// never a truncation (INV-092).
    pub fn select(&self, request: &DesignRequest) -> Result<DesignPack, DesignSelectError> {
        let mut chosen: Vec<(&DesignModule, &DesignSection, bool)> = Vec::new();
        let mut used: BTreeSet<String> = BTreeSet::new();
        let mut tokens = 0_u64;

        for id in &request.pinned {
            let (module, section) = self
                .locate(id)
                .ok_or_else(|| DesignSelectError::UnknownPin { id: id.clone() })?;
            if used.contains(&section.id) {
                continue;
            }
            tokens += section.tokens;
            if tokens > request.budget_tokens {
                return Err(DesignSelectError::PinnedOverflow {
                    id: section.id.clone(),
                    tokens,
                    budget: request.budget_tokens,
                });
            }
            used.insert(section.id.clone());
            chosen.push((module, section, true));
        }

        let intent: Vec<String> = request
            .intent_tags
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect();
        let domains = request.surface.domains();
        let mut ranked: Vec<(u32, usize, usize)> = Vec::new();
        for (module_idx, module) in self.modules.iter().enumerate() {
            let domain_bonus = if domains.contains(&module.domain.as_str()) {
                4
            } else {
                0
            };
            for (section_idx, section) in module.sections.iter().enumerate() {
                if used.contains(&section.id) {
                    continue;
                }
                let title_words = split_words(&section.title);
                let mut score = domain_bonus;
                for tag in &intent {
                    if module.tags.iter().any(|t| term_matches(tag, t)) {
                        score += 3;
                    }
                    if title_words.iter().any(|t| term_matches(tag, t)) {
                        score += 2;
                    }
                }
                if let Some(pack) = &request.style_pack {
                    if module.id == "art-direction/styles" && &section.section == pack {
                        score += 6;
                    } else if module.tags.iter().any(|t| t == pack) {
                        score += 2;
                    }
                }
                if score > 0 {
                    ranked.push((score, module_idx, section_idx));
                }
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

        let mut left_out = Vec::new();
        for (_, module_idx, section_idx) in ranked {
            let module = &self.modules[module_idx];
            let section = &module.sections[section_idx];
            if chosen.len() >= request.max_sections
                || tokens + section.tokens > request.budget_tokens
            {
                left_out.push(section.id.clone());
                continue;
            }
            tokens += section.tokens;
            used.insert(section.id.clone());
            chosen.push((module, section, false));
        }

        let mut text = String::from("## Design pack for this turn\n");
        let mut sections = Vec::with_capacity(chosen.len());
        for (module, section, pinned) in chosen {
            text.push_str(&format!(
                "### {} › {} (`{}`)\n{}\n",
                module.title, section.title, section.id, section.body
            ));
            sections.push(PackedSection {
                id: section.id.clone(),
                title: section.title.clone(),
                tokens: section.tokens,
                pinned,
            });
        }
        if !left_out.is_empty() {
            let named: Vec<&str> = left_out.iter().take(8).map(String::as_str).collect();
            text.push_str(&format!(
                "Not included (ask with design_query): {}\n",
                named.join(", ")
            ));
        }
        Ok(DesignPack {
            sections,
            tokens,
            left_out,
            text,
        })
    }

    /// Answer a mid-turn query, capped at `DESIGN_QUERY_ANSWER_TOKEN_BUDGET`.
    #[must_use]
    pub fn answer(&self, query: &DesignQuery, taste: Option<&TasteProfile>) -> DesignAnswer {
        let text = match query {
            DesignQuery::Section { id } => match self.section(id) {
                Some(section) => section.render(),
                None => {
                    let hits = self.search(&SearchQuery::new(id.replace(['#', '/', '-'], " ")));
                    let mut text = format!("No design section `{id}`.");
                    if !hits.is_empty() {
                        text.push_str(" Nearest: ");
                        let ids: Vec<&str> = hits.iter().take(4).map(|h| h.id.as_str()).collect();
                        text.push_str(&ids.join(", "));
                    }
                    text
                }
            },
            DesignQuery::Search { q, domain } => {
                let hits = self.search(&SearchQuery {
                    text: q.clone(),
                    domain: domain.clone(),
                    limit: DESIGN_SEARCH_MAX_HITS,
                });
                if hits.is_empty() {
                    format!("No design section matches {q:?}. Try other words, or read the index.")
                } else {
                    let mut text = String::from("Design sections (ask for one by id):\n");
                    for hit in hits {
                        text.push_str(&format!("- `{}` — {} — {}\n", hit.id, hit.title, hit.when));
                    }
                    text
                }
            }
            DesignQuery::Style { id } => {
                let pack = id.trim().to_lowercase();
                match self.section(&format!("art-direction/styles#{pack}")) {
                    Some(section) => section.render(),
                    None => {
                        let packs: Vec<&str> = self
                            .module("art-direction/styles")
                            .map(|m| m.sections.iter().map(|s| s.section.as_str()).collect())
                            .unwrap_or_default();
                        format!("No style pack `{pack}`. Packs: {}", packs.join(", "))
                    }
                }
            }
            DesignQuery::Fonts { mood, surface } => {
                self.fonts_answer(mood.as_deref(), surface.as_deref())
            }
            DesignQuery::Taste => taste
                .map(|profile| profile.render(TASTE_PROFILE_TOKEN_BUDGET))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "No taste profile yet for this project.".to_owned()),
        };
        let (text, capped) = cap_text(text, DESIGN_QUERY_ANSWER_TOKEN_BUDGET);
        DesignAnswer { text, capped }
    }

    fn fonts_answer(&self, mood: Option<&str>, surface: Option<&str>) -> String {
        let Some(section) = self.section("web/fonts#pairings") else {
            return "The font pairings section is missing from the base.".to_owned();
        };
        let mut text = String::new();
        match mood.map(str::to_lowercase).filter(|m| !m.trim().is_empty()) {
            Some(mood) => {
                let words = query_words(&mood);
                let rows: Vec<&str> = section
                    .body
                    .lines()
                    .filter(|line| line.starts_with('|'))
                    .collect();
                let matched: Vec<&str> = rows
                    .iter()
                    .skip(2)
                    .copied()
                    .filter(|row| {
                        let lower = row.to_lowercase();
                        words.iter().any(|w| lower.contains(w.as_str()))
                    })
                    .collect();
                if matched.is_empty() {
                    text.push_str(&format!(
                        "No pairing is filed under {mood:?}; the full table:\n"
                    ));
                    text.push_str(&section.render());
                } else {
                    text.push_str(&format!(
                        "Font pairings for {mood:?} (display / body / utility):\n"
                    ));
                    for row in rows.iter().take(2) {
                        text.push_str(row);
                        text.push('\n');
                    }
                    for row in matched.iter().take(3) {
                        text.push_str(row);
                        text.push('\n');
                    }
                }
            }
            None => text.push_str(&section.render()),
        }
        if surface.is_some_and(|s| s.to_lowercase().contains("game")) {
            text.push_str(
                "\nIn Godot the face is a .ttf/.otf under assets/fonts/ with a licence sidecar, set on the Theme with MSDF on for scaling text — see `game-ui/godot-control#fonts`.\n",
            );
        }
        text
    }

    fn locate(&self, id: &str) -> Option<(&DesignModule, &DesignSection)> {
        let (module_id, section_id) = id.split_once('#')?;
        let module = self.module(module_id)?;
        let section = module
            .sections
            .iter()
            .find(|section| section.section == section_id)?;
        Some((module, section))
    }
}

fn parse_index(source: &str) -> Result<Vec<String>, DesignError> {
    let mut ids = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("- `") else {
            continue;
        };
        let Some(end) = rest.find('`') else {
            continue;
        };
        let id = rest[..end].to_owned();
        if ids.contains(&id) {
            return Err(DesignError::IndexDuplicate { module: id });
        }
        ids.push(id);
    }
    Ok(ids)
}

fn parse_module(id: &str, source: &str) -> Result<DesignModule, DesignError> {
    let module = id.to_owned();
    let mut lines = source.lines();
    let mut header: BTreeMap<String, String> = BTreeMap::new();
    for line in lines.by_ref() {
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(DesignError::MalformedHeader {
                module,
                line: line.to_owned(),
            });
        };
        header.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    let take = |key: &'static str| -> Result<String, DesignError> {
        header
            .get(key)
            .filter(|value| !value.is_empty())
            .cloned()
            .ok_or(DesignError::MissingHeaderKey {
                module: id.to_owned(),
                key,
            })
    };
    let version_text = take("version")?;
    let version: u32 = version_text
        .parse()
        .map_err(|_| DesignError::MalformedHeader {
            module: module.clone(),
            line: format!("version: {version_text}"),
        })?;
    if version > DESIGN_KB_MAJOR {
        return Err(DesignError::UnsupportedMajor { module, version });
    }
    let domain_text = take("domain")?;
    let domain = DesignDomain::parse(&domain_text).ok_or_else(|| DesignError::UnknownDomain {
        module: module.clone(),
        domain: domain_text.clone(),
    })?;
    if id.split('/').next() != Some(domain.as_str()) {
        return Err(DesignError::DomainMismatch {
            module,
            header: domain_text,
        });
    }
    let title = take("title")?;
    let when = take("when")?;
    let mut tags = Vec::new();
    for tag in take("tags")?.split(',') {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() {
            continue;
        }
        if tags.contains(&tag) {
            return Err(DesignError::DuplicateTag { module, tag });
        }
        tags.push(tag);
    }

    let mut intro = String::new();
    let mut sections: Vec<DesignSection> = Vec::new();
    let mut current: Option<(String, Option<String>, String)> = None;

    let flush = |current: &mut Option<(String, Option<String>, String)>,
                 sections: &mut Vec<DesignSection>|
     -> Result<(), DesignError> {
        if let Some((section, title, body)) = current.take() {
            let title = title.ok_or_else(|| DesignError::MissingHeading {
                module: id.to_owned(),
                section: section.clone(),
            })?;
            let body = body.trim().to_owned();
            let full_id = format!("{id}#{section}");
            let tokens = estimate_text_tokens(&format!("## {title}\n{body}\n"));
            sections.push(DesignSection {
                id: full_id,
                section,
                title,
                body,
                tokens,
            });
        }
        Ok(())
    };

    for line in lines {
        if let Some(section) = section_marker(line) {
            flush(&mut current, &mut sections)?;
            if section.is_empty()
                || !section
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return Err(DesignError::BadSectionId {
                    module,
                    section: section.to_owned(),
                });
            }
            if sections.iter().any(|s| s.section == section) {
                return Err(DesignError::DuplicateSection {
                    module,
                    section: section.to_owned(),
                });
            }
            current = Some((section.to_owned(), None, String::new()));
        } else if let Some((section, title, body)) = current.as_mut() {
            if title.is_none() {
                if line.trim().is_empty() {
                    continue;
                }
                let Some(heading) = line.trim_start().strip_prefix('#') else {
                    return Err(DesignError::MissingHeading {
                        module,
                        section: section.clone(),
                    });
                };
                *title = Some(heading.trim_start_matches('#').trim().to_owned());
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else {
            intro.push_str(line);
            intro.push('\n');
        }
    }
    flush(&mut current, &mut sections)?;
    if sections.is_empty() {
        return Err(DesignError::NoSections { module });
    }
    let tokens = estimate_text_tokens(source);
    Ok(DesignModule {
        id: module,
        version,
        domain,
        title,
        when,
        tags,
        intro: intro.trim().to_owned(),
        sections,
        tokens,
    })
}

fn section_marker(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix("<!-- section:")?.strip_suffix("-->")?;
    Some(inner.trim())
}

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "of", "for", "and", "or", "to", "in", "on", "at", "with", "is", "it", "my",
    "me", "make", "more", "less", "better", "how", "do", "i", "this", "that", "be", "should",
    "can", "what", "when", "use", "using", "our", "your",
];

fn split_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .map(|w| w.trim_matches('-'))
        .filter(|w| w.len() >= 2)
        .map(str::to_owned)
        .collect()
}

fn query_words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = split_words(text)
        .into_iter()
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .collect();
    words.dedup();
    words
}

/// A query word matches a term when they are equal or one is a ≥ 4-character prefix of the
/// other: `light` reaches `lighting`, `type` reaches `typography`, `states` reaches `state`.
fn term_matches(word: &str, term: &str) -> bool {
    if word == term {
        return true;
    }
    (word.len() >= 4 && term.starts_with(word)) || (term.len() >= 4 && word.starts_with(term))
}

fn cap_text(mut text: String, budget: u64) -> (String, bool) {
    if estimate_text_tokens(&text) <= budget {
        return (text, false);
    }
    let max_bytes =
        usize::try_from(budget.saturating_mul(crate::context::ESTIMATED_BYTES_PER_TOKEN))
            .unwrap_or(usize::MAX)
            .min(text.len());
    let mut boundary = max_bytes;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text.push_str("\n…answer capped; ask for a narrower section.\n");
    (text, true)
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Taste: the semantic memory
// ─────────────────────────────────────────────────────────────────────────────────────────

/// Where a pin came from. The derive order is the rank: a weaker origin never replaces a
/// stronger one on the same key.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TasteOrigin {
    /// Rust read a pattern from several acceptances.
    Inferred,
    /// A verified design decision the user moved on from.
    Accepted,
    /// The user changed what the model chose.
    Corrected,
    /// The user said so, in their own words.
    Stated,
}

impl TasteOrigin {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inferred => "inferred",
            Self::Accepted => "accepted",
            Self::Corrected => "corrected",
            Self::Stated => "stated",
        }
    }

    const fn max_weight(self) -> f32 {
        match self {
            Self::Inferred => 0.5,
            Self::Accepted => 0.8,
            Self::Corrected => 0.9,
            Self::Stated => 1.0,
        }
    }
}

/// One pinned preference.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TastePin {
    pub key: String,
    pub value: String,
    pub origin: TasteOrigin,
    pub weight: f32,
    pub evidence: u32,
    pub last_seen: DateTime<Utc>,
}

/// A value the loop must not pick again for a key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TasteAvoid {
    pub key: String,
    pub value: String,
    pub reason: String,
}

/// A typed event the loop learns from. Free text never reaches the profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TasteSignal {
    Stated {
        key: String,
        value: String,
    },
    Corrected {
        key: String,
        from: String,
        to: String,
    },
    Accepted {
        key: String,
        value: String,
    },
    Inferred {
        key: String,
        value: String,
    },
    Undone {
        key: String,
        value: String,
    },
}

/// What a signal did to the profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TasteChange {
    Pinned,
    Reinforced,
    Replaced,
    Weakened,
    Removed,
    Avoided,
    Ignored,
}

/// The semantic memory: what this user likes, with provenance (docs/18 §6.2).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TasteProfile {
    pub format: String,
    pub pins: Vec<TastePin>,
    pub avoid: Vec<TasteAvoid>,
    /// Undo strikes per `key\u{1f}value`; three make an avoid.
    #[serde(default)]
    strikes: BTreeMap<String, u32>,
}

impl Default for TasteProfile {
    fn default() -> Self {
        Self::new()
    }
}

impl TasteProfile {
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: TASTE_FORMAT.to_owned(),
            pins: Vec::new(),
            avoid: Vec::new(),
            strikes: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn pin(&self, key: &str) -> Option<&TastePin> {
        self.pins.iter().find(|pin| pin.key == key)
    }

    #[must_use]
    pub fn is_avoided(&self, key: &str, value: &str) -> bool {
        self.avoid
            .iter()
            .any(|avoid| avoid.key == key && avoid.value == value)
    }

    /// Bounded and typed: the format is known, every key and value is a short projection,
    /// every weight is in range.
    pub fn validate(&self) -> Result<(), String> {
        if self.format != TASTE_FORMAT {
            return Err(format!(
                "taste profile format `{}` is not {TASTE_FORMAT}",
                self.format
            ));
        }
        for pin in &self.pins {
            if pin.key.is_empty() || pin.key.len() > 64 {
                return Err(format!("taste key `{}` must be 1–64 bytes", pin.key));
            }
            if pin.value.is_empty() || pin.value.len() > MAX_PROJECTION_BYTES {
                return Err(format!(
                    "taste value for `{}` must be 1–{MAX_PROJECTION_BYTES} bytes",
                    pin.key
                ));
            }
            if !(0.0..=1.0).contains(&pin.weight) {
                return Err(format!("taste weight for `{}` is out of range", pin.key));
            }
        }
        Ok(())
    }

    /// Apply one signal by the deterministic rules of docs/18 §6.2.
    pub fn apply(&mut self, signal: &TasteSignal, at: DateTime<Utc>) -> TasteChange {
        let change = match signal {
            TasteSignal::Stated { key, value } => {
                self.avoid.retain(|a| !(a.key == *key && a.value == *value));
                match self.pin_index(key) {
                    Some(idx) => {
                        let same = self.pins[idx].value == *value;
                        let evidence = if same { self.pins[idx].evidence + 1 } else { 1 };
                        self.pins[idx] = TastePin {
                            key: key.clone(),
                            value: value.clone(),
                            origin: TasteOrigin::Stated,
                            weight: 1.0,
                            evidence,
                            last_seen: at,
                        };
                        if same {
                            TasteChange::Reinforced
                        } else {
                            TasteChange::Replaced
                        }
                    }
                    None => {
                        self.push_pin(key, value, TasteOrigin::Stated, 1.0, at);
                        TasteChange::Pinned
                    }
                }
            }
            TasteSignal::Corrected { key, from, to } => {
                if from != to && !self.stated_value_is(key, from) {
                    self.add_avoid(key, from, "corrected away");
                }
                match self.pin_index(key) {
                    Some(idx) if self.pins[idx].origin > TasteOrigin::Corrected => {
                        if self.pins[idx].value == *to {
                            self.pins[idx].evidence += 1;
                            self.pins[idx].last_seen = at;
                            TasteChange::Reinforced
                        } else {
                            TasteChange::Ignored
                        }
                    }
                    Some(idx) => {
                        let same = self.pins[idx].value == *to;
                        let weight = if same {
                            (self.pins[idx].weight + 0.1).min(TasteOrigin::Corrected.max_weight())
                        } else {
                            0.8
                        };
                        let evidence = if same { self.pins[idx].evidence + 1 } else { 1 };
                        self.pins[idx] = TastePin {
                            key: key.clone(),
                            value: to.clone(),
                            origin: TasteOrigin::Corrected,
                            weight,
                            evidence,
                            last_seen: at,
                        };
                        if same {
                            TasteChange::Reinforced
                        } else {
                            TasteChange::Replaced
                        }
                    }
                    None => {
                        self.push_pin(key, to, TasteOrigin::Corrected, 0.8, at);
                        TasteChange::Pinned
                    }
                }
            }
            TasteSignal::Accepted { key, value } => {
                if self.is_avoided(key, value) {
                    TasteChange::Ignored
                } else {
                    match self.pin_index(key) {
                        None => {
                            self.push_pin(key, value, TasteOrigin::Accepted, 0.2, at);
                            TasteChange::Pinned
                        }
                        Some(idx) if self.pins[idx].value == *value => {
                            let pin = &mut self.pins[idx];
                            if pin.origin < TasteOrigin::Accepted {
                                pin.origin = TasteOrigin::Accepted;
                            }
                            pin.weight = (pin.weight + 0.2).min(pin.origin.max_weight());
                            pin.evidence += 1;
                            pin.last_seen = at;
                            TasteChange::Reinforced
                        }
                        Some(idx) if self.pins[idx].origin < TasteOrigin::Accepted => {
                            self.pins[idx] = TastePin {
                                key: key.clone(),
                                value: value.clone(),
                                origin: TasteOrigin::Accepted,
                                weight: 0.2,
                                evidence: 1,
                                last_seen: at,
                            };
                            TasteChange::Replaced
                        }
                        Some(_) => TasteChange::Ignored,
                    }
                }
            }
            TasteSignal::Inferred { key, value } => {
                if self.is_avoided(key, value) {
                    TasteChange::Ignored
                } else {
                    match self.pin_index(key) {
                        None => {
                            self.push_pin(key, value, TasteOrigin::Inferred, 0.3, at);
                            TasteChange::Pinned
                        }
                        Some(idx) if self.pins[idx].value == *value => {
                            let pin = &mut self.pins[idx];
                            if pin.origin == TasteOrigin::Inferred {
                                pin.weight =
                                    (pin.weight + 0.1).min(TasteOrigin::Inferred.max_weight());
                            }
                            pin.evidence += 1;
                            pin.last_seen = at;
                            TasteChange::Reinforced
                        }
                        Some(idx)
                            if self.pins[idx].origin == TasteOrigin::Inferred
                                && self.pins[idx].weight <= 0.3 + f32::EPSILON =>
                        {
                            self.pins[idx] = TastePin {
                                key: key.clone(),
                                value: value.clone(),
                                origin: TasteOrigin::Inferred,
                                weight: 0.3,
                                evidence: 1,
                                last_seen: at,
                            };
                            TasteChange::Replaced
                        }
                        Some(_) => TasteChange::Ignored,
                    }
                }
            }
            TasteSignal::Undone { key, value } => {
                let strikes = self.strike(key, value);
                match self.pin_index(key) {
                    Some(idx) if self.pins[idx].value == *value => {
                        if self.pins[idx].origin == TasteOrigin::Stated {
                            TasteChange::Ignored
                        } else if strikes >= 3 {
                            self.pins.remove(idx);
                            self.add_avoid(key, value, "undone three times");
                            TasteChange::Avoided
                        } else {
                            self.pins[idx].weight -= 0.3;
                            if self.pins[idx].weight <= 0.0 {
                                self.pins.remove(idx);
                                TasteChange::Removed
                            } else {
                                TasteChange::Weakened
                            }
                        }
                    }
                    _ if strikes >= 3 && !self.stated_value_is(key, value) => {
                        self.add_avoid(key, value, "undone three times");
                        TasteChange::Avoided
                    }
                    _ => TasteChange::Ignored,
                }
            }
        };
        self.enforce_cap();
        change
    }

    /// This profile laid over a base one (a project profile over the user's): every key
    /// here wins; avoids are the union.
    #[must_use]
    pub fn merged_over(&self, base: &Self) -> Self {
        let mut merged = base.clone();
        for pin in &self.pins {
            match merged.pin_index(&pin.key) {
                Some(idx) => merged.pins[idx] = pin.clone(),
                None => merged.pins.push(pin.clone()),
            }
        }
        for avoid in &self.avoid {
            if !merged.is_avoided(&avoid.key, &avoid.value) {
                merged.avoid.push(avoid.clone());
            }
        }
        merged.format = TASTE_FORMAT.to_owned();
        merged
    }

    /// The block the model reads: strongest first, under a budget, and honest about what
    /// was left out.
    #[must_use]
    pub fn render(&self, budget: u64) -> String {
        if self.pins.is_empty() && self.avoid.is_empty() {
            return String::new();
        }
        let mut pins: Vec<&TastePin> = self.pins.iter().collect();
        pins.sort_by(|a, b| {
            b.origin
                .cmp(&a.origin)
                .then(b.weight.partial_cmp(&a.weight).unwrap_or(Ordering::Equal))
                .then(a.key.cmp(&b.key))
        });
        let mut lines: Vec<String> = pins
            .iter()
            .map(|pin| format!("- {}: {} ({})", pin.key, pin.value, pin.origin.as_str()))
            .collect();
        lines.extend(
            self.avoid
                .iter()
                .map(|avoid| format!("- avoid {}: {}", avoid.key, avoid.value)),
        );
        let mut out = String::from("## Taste\n");
        let mut shown = 0_usize;
        for line in &lines {
            if estimate_text_tokens(&format!("{out}{line}\n")) > budget {
                break;
            }
            out.push_str(line);
            out.push('\n');
            shown += 1;
        }
        if shown < lines.len() {
            out.push_str(&format!(
                "(+{} more; ask with design_query taste)\n",
                lines.len() - shown
            ));
        }
        out
    }

    fn pin_index(&self, key: &str) -> Option<usize> {
        self.pins.iter().position(|pin| pin.key == key)
    }

    fn stated_value_is(&self, key: &str, value: &str) -> bool {
        self.pin(key)
            .is_some_and(|pin| pin.origin == TasteOrigin::Stated && pin.value == value)
    }

    fn push_pin(
        &mut self,
        key: &str,
        value: &str,
        origin: TasteOrigin,
        weight: f32,
        at: DateTime<Utc>,
    ) {
        self.pins.push(TastePin {
            key: key.to_owned(),
            value: value.to_owned(),
            origin,
            weight,
            evidence: 1,
            last_seen: at,
        });
    }

    fn add_avoid(&mut self, key: &str, value: &str, reason: &str) {
        if !self.is_avoided(key, value) {
            self.avoid.push(TasteAvoid {
                key: key.to_owned(),
                value: value.to_owned(),
                reason: reason.to_owned(),
            });
        }
    }

    fn strike(&mut self, key: &str, value: &str) -> u32 {
        let entry = self
            .strikes
            .entry(format!("{key}\u{1f}{value}"))
            .or_insert(0);
        *entry += 1;
        *entry
    }

    /// Evict the weakest non-stated pin until the cap holds. A stated pin is never evicted.
    fn enforce_cap(&mut self) {
        while self.pins.len() > TASTE_PROFILE_MAX_PINS {
            let victim = self
                .pins
                .iter()
                .enumerate()
                .filter(|(_, pin)| pin.origin != TasteOrigin::Stated)
                .min_by(|(_, a), (_, b)| {
                    a.origin
                        .cmp(&b.origin)
                        .then(a.weight.partial_cmp(&b.weight).unwrap_or(Ordering::Equal))
                        .then(a.last_seen.cmp(&b.last_seen))
                })
                .map(|(idx, _)| idx);
            match victim {
                Some(idx) => {
                    self.pins.remove(idx);
                }
                None => break,
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Lessons: the procedural memory
// ─────────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    Proposed,
    Approved,
    Rejected,
}

/// A rule with trigger tags, distilled from episodes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignLesson {
    pub id: String,
    pub domain: String,
    pub trigger_tags: Vec<String>,
    pub rule: String,
    /// Episode ids that support the rule. Never fewer than `DESIGN_LESSON_MIN_EVIDENCE`.
    pub evidence: Vec<String>,
    pub status: LessonStatus,
    pub hits: u32,
    pub misses: u32,
    pub proposed_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

/// What Rust or the model proposes; the book validates it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonDraft {
    pub domain: String,
    pub trigger_tags: Vec<String>,
    pub rule: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LessonError {
    NotEnoughEvidence { have: usize, need: usize },
    EmptyRule,
    RuleTooLong { bytes: usize, max: usize },
    NoTriggerTags,
    PreviouslyRejected { id: String },
    Duplicate { id: String },
    UnknownLesson { id: String },
    NotProposed { id: String },
    TooManyApproved { max: usize },
}

impl fmt::Display for LessonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughEvidence { have, need } => {
                write!(
                    f,
                    "a lesson needs {need} episodes of evidence; {have} given"
                )
            }
            Self::EmptyRule => write!(f, "a lesson needs a rule"),
            Self::RuleTooLong { bytes, max } => {
                write!(
                    f,
                    "a lesson's rule is one sentence: {bytes} bytes given, {max} allowed"
                )
            }
            Self::NoTriggerTags => write!(f, "a lesson needs at least one trigger tag"),
            Self::PreviouslyRejected { id } => {
                write!(
                    f,
                    "lesson `{id}` was rejected by the user and is not re-proposed"
                )
            }
            Self::Duplicate { id } => write!(f, "lesson `{id}` already exists"),
            Self::UnknownLesson { id } => write!(f, "no lesson `{id}`"),
            Self::NotProposed { id } => write!(f, "lesson `{id}` is not awaiting a decision"),
            Self::TooManyApproved { max } => {
                write!(
                    f,
                    "{max} lessons are already approved; consolidate before approving more"
                )
            }
        }
    }
}

impl std::error::Error for LessonError {}

/// The procedural memory (docs/18 §6.3): proposed → approved | rejected, by the user only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LessonBook {
    pub format: String,
    pub lessons: Vec<DesignLesson>,
}

impl Default for LessonBook {
    fn default() -> Self {
        Self::new()
    }
}

impl LessonBook {
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: LESSON_FORMAT.to_owned(),
            lessons: Vec::new(),
        }
    }

    /// The id a draft would get: stable for the same domain and rule, so a rejected lesson
    /// stays rejected however it is re-worded in whitespace or case.
    #[must_use]
    pub fn id_for(domain: &str, rule: &str) -> String {
        let normalised = format!(
            "{}\u{1f}{}",
            domain.trim().to_lowercase(),
            rule.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        );
        let hash = blake3::hash(normalised.as_bytes()).to_hex();
        format!("lesson_{}", &hash.as_str()[..16])
    }

    /// Stage a lesson for the user's decision. Returns its id.
    pub fn propose(
        &mut self,
        draft: LessonDraft,
        at: DateTime<Utc>,
    ) -> Result<String, LessonError> {
        let rule = draft.rule.trim();
        if rule.is_empty() {
            return Err(LessonError::EmptyRule);
        }
        if rule.len() > DESIGN_LESSON_MAX_RULE_BYTES {
            return Err(LessonError::RuleTooLong {
                bytes: rule.len(),
                max: DESIGN_LESSON_MAX_RULE_BYTES,
            });
        }
        let mut evidence: Vec<String> = draft
            .evidence
            .iter()
            .map(|e| e.trim().to_owned())
            .filter(|e| !e.is_empty())
            .collect();
        evidence.sort();
        evidence.dedup();
        if evidence.len() < DESIGN_LESSON_MIN_EVIDENCE {
            return Err(LessonError::NotEnoughEvidence {
                have: evidence.len(),
                need: DESIGN_LESSON_MIN_EVIDENCE,
            });
        }
        let tags: Vec<String> = draft
            .trigger_tags
            .iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if tags.is_empty() {
            return Err(LessonError::NoTriggerTags);
        }
        let id = Self::id_for(&draft.domain, rule);
        if let Some(existing) = self.lessons.iter().find(|l| l.id == id) {
            return Err(match existing.status {
                LessonStatus::Rejected => LessonError::PreviouslyRejected { id },
                _ => LessonError::Duplicate { id },
            });
        }
        self.lessons.push(DesignLesson {
            id: id.clone(),
            domain: draft.domain.trim().to_lowercase(),
            trigger_tags: tags,
            rule: rule.to_owned(),
            evidence,
            status: LessonStatus::Proposed,
            hits: 0,
            misses: 0,
            proposed_at: at,
            decided_at: None,
        });
        Ok(id)
    }

    /// The user's *Keep*.
    pub fn approve(&mut self, id: &str, at: DateTime<Utc>) -> Result<(), LessonError> {
        let approved = self
            .lessons
            .iter()
            .filter(|l| l.status == LessonStatus::Approved)
            .count();
        let lesson = self
            .lessons
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| LessonError::UnknownLesson { id: id.to_owned() })?;
        if lesson.status != LessonStatus::Proposed {
            return Err(LessonError::NotProposed { id: id.to_owned() });
        }
        if approved >= DESIGN_LESSONS_MAX_APPROVED {
            return Err(LessonError::TooManyApproved {
                max: DESIGN_LESSONS_MAX_APPROVED,
            });
        }
        lesson.status = LessonStatus::Approved;
        lesson.decided_at = Some(at);
        Ok(())
    }

    /// The user's *Never* (or a later *Forget this one*). Remembered so it is not re-proposed.
    pub fn reject(&mut self, id: &str, at: DateTime<Utc>) -> Result<(), LessonError> {
        let lesson = self
            .lessons
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| LessonError::UnknownLesson { id: id.to_owned() })?;
        lesson.status = LessonStatus::Rejected;
        lesson.decided_at = Some(at);
        Ok(())
    }

    /// Approved lessons whose trigger tags overlap the turn's tags. Proposed and rejected
    /// lessons never match (INV-094).
    #[must_use]
    pub fn matching(&self, tags: &[String]) -> Vec<&DesignLesson> {
        let tags: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
        self.lessons
            .iter()
            .filter(|l| l.status == LessonStatus::Approved)
            .filter(|l| {
                l.trigger_tags
                    .iter()
                    .any(|trigger| tags.iter().any(|tag| term_matches(tag, trigger)))
            })
            .collect()
    }

    /// Count whether an injected lesson changed the outcome.
    pub fn record(&mut self, id: &str, hit: bool) -> Result<(), LessonError> {
        let lesson = self
            .lessons
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| LessonError::UnknownLesson { id: id.to_owned() })?;
        if hit {
            lesson.hits += 1;
        } else {
            lesson.misses += 1;
        }
        Ok(())
    }

    #[must_use]
    pub fn proposed(&self) -> Vec<&DesignLesson> {
        self.lessons
            .iter()
            .filter(|l| l.status == LessonStatus::Proposed)
            .collect()
    }

    /// The block the model reads: approved, matching, under a budget.
    #[must_use]
    pub fn render(&self, tags: &[String], budget: u64) -> String {
        let matching = self.matching(tags);
        if matching.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Lessons (approved by the user)\n");
        let mut shown = 0_usize;
        for lesson in &matching {
            let line = format!("- {}\n", lesson.rule);
            if estimate_text_tokens(&format!("{out}{line}")) > budget {
                break;
            }
            out.push_str(&line);
            shown += 1;
        }
        if shown < matching.len() {
            out.push_str(&format!("(+{} more)\n", matching.len() - shown));
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Episodes: the projection the loop stores
// ─────────────────────────────────────────────────────────────────────────────────────────

/// How the user reacted to what was built.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeReaction {
    Accepted,
    Corrected,
    Undone,
    Praised,
    #[default]
    Unknown,
}

/// What happened, as counts, ids and hashes — never a transcript, a screenshot or a file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignEpisode {
    pub format: String,
    pub id: String,
    pub surface: DesignSurface,
    /// Hash of the design plan or brief the surface was built against.
    pub plan_hash: String,
    /// Section ids the pack carried for the turn.
    pub sections: Vec<String>,
    /// Evidence ids (the typed probe and the visual observation).
    pub evidence: Vec<String>,
    /// The critique's total, if a visual half was judged.
    pub critique_total: Option<u8>,
    pub reaction: EpisodeReaction,
    /// One sentence at most: the critique's `worst.fix` or the correction's key.
    pub note: String,
    pub at: DateTime<Utc>,
}

impl DesignEpisode {
    /// Bounded: short ids, a short note, no room for a transcript (INV-094).
    pub fn validate(&self) -> Result<(), String> {
        if self.format != DESIGN_EPISODE_FORMAT {
            return Err(format!(
                "episode format `{}` is not {DESIGN_EPISODE_FORMAT}",
                self.format
            ));
        }
        if self.id.is_empty() || self.id.len() > 64 {
            return Err("episode id must be 1–64 bytes".to_owned());
        }
        if self.plan_hash.len() > 128 {
            return Err("episode plan hash is too long".to_owned());
        }
        if self.note.len() > MAX_PROJECTION_BYTES {
            return Err(format!(
                "episode note must stay under {MAX_PROJECTION_BYTES} bytes"
            ));
        }
        if self.sections.len() > 16 || self.evidence.len() > 16 {
            return Err("episode carries too many ids".to_owned());
        }
        if self.critique_total.is_some_and(|total| total > 30) {
            return Err("critique total is out of 30".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use bhippi_types::{DESIGN_INDEX_TOKEN_BUDGET, DESIGN_LESSON_TOKEN_BUDGET};
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap()
    }

    fn kb() -> DesignKb {
        DesignKb::bundled().expect("the bundled base parses")
    }

    const GOOD: &str = "version: 1\ndomain: web\ntitle: T\nwhen: w\ntags: a, b\n\n# T\n<!-- section: one -->\n## One\nbody one\n<!-- section: two -->\n## Two\nbody two\n";
    const INDEX: &str = "- `web/t`\n";

    #[test]
    fn bundled_parses_with_sections_and_tags() {
        let kb = kb();
        assert_eq!(kb.modules().len(), MODULE_SOURCES.len());
        for module in kb.modules() {
            assert!(!module.sections.is_empty(), "{} has sections", module.id);
            assert!(!module.tags.is_empty(), "{} has tags", module.id);
            assert!(!module.when.is_empty(), "{} has a when line", module.id);
            assert_eq!(module.version, DESIGN_KB_MAJOR);
        }
    }

    #[test]
    fn section_ids_are_unique_and_resolvable() {
        let kb = kb();
        let mut seen = BTreeSet::new();
        for module in kb.modules() {
            for section in &module.sections {
                assert!(seen.insert(section.id.clone()), "duplicate {}", section.id);
                assert!(kb.section(&section.id).is_some());
            }
        }
        assert!(kb.section("scene-3d/model-selection#scoring").is_some());
        assert!(kb.section("art-direction/styles#low-poly-toy").is_some());
        assert!(kb.section("nope#nope").is_none());
    }

    #[test]
    fn index_render_stays_a_map() {
        let kb = kb();
        let tokens = kb.index_tokens();
        assert!(
            tokens <= DESIGN_INDEX_TOKEN_BUDGET,
            "index is {tokens} tokens, budget {DESIGN_INDEX_TOKEN_BUDGET}"
        );
        let text = kb.render_index();
        assert!(text.contains("`scene-3d/model-selection`"));
        assert!(text.contains("### foundations"));
    }

    #[test]
    fn future_major_is_refused() {
        let src = GOOD.replacen("version: 1", "version: 2", 1);
        let err = DesignKb::parse(INDEX, &[("web/t", &src)]).unwrap_err();
        assert!(matches!(
            err,
            DesignError::UnsupportedMajor { version: 2, .. }
        ));
    }

    #[test]
    fn missing_heading_is_refused() {
        let src = GOOD.replacen("## One\n", "not a heading\n", 1);
        let err = DesignKb::parse(INDEX, &[("web/t", &src)]).unwrap_err();
        assert!(matches!(err, DesignError::MissingHeading { .. }));
    }

    #[test]
    fn duplicate_section_is_refused() {
        let src = GOOD.replacen("section: two", "section: one", 1);
        let err = DesignKb::parse(INDEX, &[("web/t", &src)]).unwrap_err();
        assert!(matches!(err, DesignError::DuplicateSection { .. }));
    }

    #[test]
    fn unknown_or_mismatched_domain_is_refused() {
        let src = GOOD.replacen("domain: web", "domain: prose", 1);
        let err = DesignKb::parse(INDEX, &[("web/t", &src)]).unwrap_err();
        assert!(matches!(err, DesignError::UnknownDomain { .. }));
        let src = GOOD.replacen("domain: web", "domain: audio", 1);
        let err = DesignKb::parse(INDEX, &[("web/t", &src)]).unwrap_err();
        assert!(matches!(err, DesignError::DomainMismatch { .. }));
    }

    #[test]
    fn index_must_cover_every_module_both_ways() {
        let err = DesignKb::parse("", &[("web/t", GOOD)]).unwrap_err();
        assert!(matches!(err, DesignError::IndexOrphan { .. }));
        let err = DesignKb::parse("- `web/t`\n- `web/u`\n", &[("web/t", GOOD)]).unwrap_err();
        assert!(matches!(err, DesignError::IndexMissing { .. }));
        let err = DesignKb::parse("- `web/t`\n- `web/t`\n", &[("web/t", GOOD)]).unwrap_err();
        assert!(matches!(err, DesignError::IndexDuplicate { .. }));
    }

    #[test]
    fn search_corpus_lands_on_the_right_module() {
        let kb = kb();
        let corpus: &[(&str, &str)] = &[
            ("health bar readable at distance", "game-ui/hud"),
            (
                "which model to place for a lamp post",
                "scene-3d/model-selection",
            ),
            (
                "how wide should a gap be for a jump",
                "scene-3d/layout-metrics",
            ),
            ("font pairing for a playful game", "web/fonts"),
            (
                "dark mode tokens prefers-color-scheme",
                "web/themes-responsive",
            ),
            ("pause menu focus gamepad", "game-ui/menus-flow"),
            (
                "fog and key light temperature",
                "scene-3d/lighting-environment",
            ),
            ("screen shake trauma", "game-ui/feedback-juice"),
            ("categorical palette validator", "web/charts"),
            ("theme resource anchors containers", "game-ui/godot-control"),
            ("pixel art tilemap parallax", "scene-2d/sprites-tiles"),
            ("propose a lesson from corrections", "learning/taste-loop"),
        ];
        for (query, expected) in corpus {
            let hits = kb.search(&SearchQuery::new(*query));
            let top = hits
                .first()
                .map(|h| h.id.split('#').next().unwrap_or(""))
                .unwrap_or("");
            assert_eq!(top, *expected, "query {query:?} → {hits:?}");
        }
    }

    #[test]
    fn search_respects_domain_filter_and_limit() {
        let kb = kb();
        let hits = kb.search(&SearchQuery {
            text: "camera".to_owned(),
            domain: Some("scene-2d".to_owned()),
            limit: 3,
        });
        assert!(!hits.is_empty());
        assert!(hits.len() <= 3);
        assert!(hits.iter().all(|h| h.id.starts_with("scene-2d/")));
        assert!(kb
            .search(&SearchQuery {
                text: "camera".to_owned(),
                domain: Some("nowhere".to_owned()),
                limit: 3,
            })
            .is_empty());
        assert!(kb.search(&SearchQuery::new("the of and")).is_empty());
    }

    #[test]
    fn select_respects_budget_and_section_cap() {
        let kb = kb();
        let request = DesignRequest::new(DesignSurface::GameUi)
            .with_tags(["hud", "health", "bar", "menu", "focus"]);
        let pack = kb.select(&request).expect("selects");
        assert!(pack.tokens <= DESIGN_CONTEXT_TOKEN_BUDGET);
        assert!(pack.sections.len() <= DESIGN_MAX_SECTIONS_PER_TURN);
        assert!(!pack.sections.is_empty());
        assert!(pack.sections.iter().all(|s| s.id.starts_with("game-ui/")
            || s.id.starts_with("art-direction/")
            || s.id.starts_with("audio/")
            || s.id.starts_with("foundations/")
            || s.id.starts_with("web/")));
        assert!(pack
            .sections
            .iter()
            .any(|s| s.id.starts_with("game-ui/hud#")));
        assert!(pack.text().starts_with("## Design pack"));
        assert!(!pack.left_out.is_empty());
        assert!(pack.text().contains("Not included"));
        let tiny = DesignRequest {
            budget_tokens: 120,
            max_sections: 1,
            ..request
        };
        let small = kb.select(&tiny).expect("selects under a tiny budget");
        assert!(small.sections.len() <= 1);
        assert!(small.tokens <= 120);
    }

    #[test]
    fn select_pins_first_and_blocks_on_pinned_overflow() {
        let kb = kb();
        let request = DesignRequest::new(DesignSurface::WebPage)
            .with_pinned(["foundations/states-a11y#floors"])
            .with_tags(["landing", "hero"]);
        let pack = kb.select(&request).expect("selects");
        assert_eq!(pack.sections[0].id, "foundations/states-a11y#floors");
        assert!(pack.sections[0].pinned);
        assert!(pack
            .sections
            .iter()
            .any(|s| s.id.starts_with("web/page-anatomy#")));

        let floors = kb.section("foundations/states-a11y#floors").unwrap().tokens;
        let starved = DesignRequest {
            budget_tokens: floors.saturating_sub(1),
            ..request.clone()
        };
        let err = kb.select(&starved).unwrap_err();
        assert!(matches!(err, DesignSelectError::PinnedOverflow { .. }));

        let unknown = DesignRequest {
            pinned: vec!["foundations/nope#nope".to_owned()],
            ..request
        };
        assert!(matches!(
            kb.select(&unknown).unwrap_err(),
            DesignSelectError::UnknownPin { .. }
        ));
    }

    #[test]
    fn select_prefers_the_surface_domain_and_the_style_pack() {
        let kb = kb();
        let scene = kb
            .select(&DesignRequest::new(DesignSurface::Scene3d).with_style_pack("neon-arcade"))
            .expect("selects");
        assert!(scene
            .sections
            .iter()
            .any(|s| s.id == "art-direction/styles#neon-arcade"));
        assert!(scene.sections.iter().any(|s| s.id.starts_with("scene-3d/")));
        let unknown = kb
            .select(&DesignRequest::new(DesignSurface::Unknown))
            .expect("selects");
        assert!(
            unknown.sections.is_empty(),
            "nothing scores with no surface and no tags"
        );
    }

    #[test]
    fn answers_are_capped_and_helpful() {
        let kb = kb();
        let section = kb.answer(
            &DesignQuery::Section {
                id: "scene-3d/model-selection#scoring".to_owned(),
            },
            None,
        );
        assert!(section.text.contains("style fit"));
        assert!(!section.capped);
        let missing = kb.answer(
            &DesignQuery::Section {
                id: "scene-3d/model-selection#nope".to_owned(),
            },
            None,
        );
        assert!(missing.text.starts_with("No design section"));
        let style = kb.answer(
            &DesignQuery::Style {
                id: "Pixel-16".to_owned(),
            },
            None,
        );
        assert!(style.text.contains("NEAREST"));
        let no_style = kb.answer(
            &DesignQuery::Style {
                id: "vaporwave".to_owned(),
            },
            None,
        );
        assert!(no_style.text.contains("low-poly-toy"));
        let fonts = kb.answer(
            &DesignQuery::Fonts {
                mood: Some("playful".to_owned()),
                surface: Some("game_ui".to_owned()),
            },
            None,
        );
        assert!(fonts.text.contains("Baloo 2"));
        assert!(fonts.text.contains("godot-control#fonts"));
        let search = kb.answer(
            &DesignQuery::Search {
                q: "safe area".to_owned(),
                domain: None,
            },
            None,
        );
        assert!(search.text.contains("game-ui/hud#"));
        let taste = kb.answer(&DesignQuery::Taste, None);
        assert!(taste.text.contains("No taste profile"));

        let (text, capped) = cap_text("x".repeat(20_000), 100);
        assert!(capped);
        assert!(text.len() < 20_000);
        assert!(text.ends_with("ask for a narrower section.\n"));
    }

    #[test]
    fn design_query_parses_the_documented_shapes() {
        let q: DesignQuery =
            serde_json::from_str(r#"{"kind":"section","id":"web/fonts#pairings"}"#).unwrap();
        assert_eq!(
            q,
            DesignQuery::Section {
                id: "web/fonts#pairings".to_owned()
            }
        );
        let q: DesignQuery = serde_json::from_str(r#"{"kind":"search","q":"hud"}"#).unwrap();
        assert_eq!(
            q,
            DesignQuery::Search {
                q: "hud".to_owned(),
                domain: None
            }
        );
        let q: DesignQuery = serde_json::from_str(r#"{"kind":"fonts","mood":"cold"}"#).unwrap();
        assert!(matches!(q, DesignQuery::Fonts { .. }));
        let q: DesignQuery = serde_json::from_str(r#"{"kind":"taste"}"#).unwrap();
        assert_eq!(q, DesignQuery::Taste);
    }

    // ── taste ────────────────────────────────────────────────────────────────────────

    fn stated(key: &str, value: &str) -> TasteSignal {
        TasteSignal::Stated {
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }
    fn inferred(key: &str, value: &str) -> TasteSignal {
        TasteSignal::Inferred {
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }
    fn accepted(key: &str, value: &str) -> TasteSignal {
        TasteSignal::Accepted {
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }
    fn undone(key: &str, value: &str) -> TasteSignal {
        TasteSignal::Undone {
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn stated_beats_inferred_and_only_a_statement_replaces_it() {
        let mut profile = TasteProfile::new();
        assert_eq!(
            profile.apply(&stated("palette.temperature", "warm"), now()),
            TasteChange::Pinned
        );
        assert_eq!(
            profile.apply(&inferred("palette.temperature", "cool"), now()),
            TasteChange::Ignored
        );
        assert_eq!(
            profile.apply(&accepted("palette.temperature", "cool"), now()),
            TasteChange::Ignored
        );
        assert_eq!(
            profile.apply(
                &TasteSignal::Corrected {
                    key: "palette.temperature".into(),
                    from: "warm".into(),
                    to: "cool".into()
                },
                now()
            ),
            TasteChange::Ignored
        );
        assert_eq!(profile.pin("palette.temperature").unwrap().value, "warm");
        assert_eq!(
            profile.apply(&stated("palette.temperature", "cool"), now()),
            TasteChange::Replaced
        );
        assert_eq!(profile.pin("palette.temperature").unwrap().value, "cool");
        assert_eq!(
            profile.pin("palette.temperature").unwrap().origin,
            TasteOrigin::Stated
        );
        profile.validate().unwrap();
    }

    #[test]
    fn corrections_add_an_avoid_and_the_avoid_blocks_reinference() {
        let mut profile = TasteProfile::new();
        let change = profile.apply(
            &TasteSignal::Corrected {
                key: "type.display".into(),
                from: "Inter".into(),
                to: "Baloo 2".into(),
            },
            now(),
        );
        assert_eq!(change, TasteChange::Pinned);
        assert!(profile.is_avoided("type.display", "Inter"));
        assert_eq!(
            profile.apply(&inferred("type.display", "Inter"), now()),
            TasteChange::Ignored
        );
        assert_eq!(
            profile.apply(&accepted("type.display", "Inter"), now()),
            TasteChange::Ignored
        );
        assert_eq!(profile.pin("type.display").unwrap().value, "Baloo 2");
        // A statement lifts the avoid.
        assert_eq!(
            profile.apply(&stated("type.display", "Inter"), now()),
            TasteChange::Replaced
        );
        assert!(!profile.is_avoided("type.display", "Inter"));
    }

    #[test]
    fn acceptances_reinforce_and_cap_and_inference_needs_a_pattern() {
        let mut profile = TasteProfile::new();
        assert_eq!(
            profile.apply(&inferred("density", "dense"), now()),
            TasteChange::Pinned
        );
        assert!((profile.pin("density").unwrap().weight - 0.3).abs() < 1e-6);
        assert_eq!(
            profile.apply(&inferred("density", "airy"), now()),
            TasteChange::Replaced
        );
        assert_eq!(
            profile.apply(&inferred("density", "airy"), now()),
            TasteChange::Reinforced
        );
        assert_eq!(
            profile.apply(&inferred("density", "dense"), now()),
            TasteChange::Ignored,
            "a reinforced inference is not flipped by one more"
        );
        for _ in 0..6 {
            profile.apply(&accepted("density", "airy"), now());
        }
        let pin = profile.pin("density").unwrap();
        assert_eq!(pin.origin, TasteOrigin::Accepted);
        assert!(pin.weight <= TasteOrigin::Accepted.max_weight() + 1e-6);
        assert!(pin.evidence >= 7);
    }

    #[test]
    fn three_undos_make_an_avoid_but_a_statement_is_never_undone() {
        let mut profile = TasteProfile::new();
        profile.apply(&accepted("motion.amount", "lots"), now());
        assert_eq!(
            profile.apply(&undone("motion.amount", "lots"), now()),
            TasteChange::Removed
        );
        assert!(profile.pin("motion.amount").is_none());
        assert_eq!(
            profile.apply(&undone("motion.amount", "lots"), now()),
            TasteChange::Ignored
        );
        assert_eq!(
            profile.apply(&undone("motion.amount", "lots"), now()),
            TasteChange::Avoided
        );
        assert!(profile.is_avoided("motion.amount", "lots"));

        let mut stated_profile = TasteProfile::new();
        stated_profile.apply(&stated("copy.tone", "terse"), now());
        for _ in 0..4 {
            assert_eq!(
                stated_profile.apply(&undone("copy.tone", "terse"), now()),
                TasteChange::Ignored
            );
        }
        assert_eq!(stated_profile.pin("copy.tone").unwrap().value, "terse");
        assert!(!stated_profile.is_avoided("copy.tone", "terse"));
    }

    #[test]
    fn the_pin_cap_evicts_inferred_first_and_never_stated() {
        let mut profile = TasteProfile::new();
        for i in 0..TASTE_PROFILE_MAX_PINS {
            profile.apply(&stated(&format!("k{i}"), "v"), now());
        }
        assert_eq!(profile.pins.len(), TASTE_PROFILE_MAX_PINS);
        profile.apply(&inferred("extra", "v"), now());
        assert_eq!(profile.pins.len(), TASTE_PROFILE_MAX_PINS);
        assert!(
            profile.pin("extra").is_none(),
            "the inferred newcomer is the one evicted"
        );
        assert!(profile.pins.iter().all(|p| p.origin == TasteOrigin::Stated));
    }

    #[test]
    fn render_is_strongest_first_and_under_budget() {
        let mut profile = TasteProfile::new();
        profile.apply(&inferred("lighting.preset", "golden-hour"), now());
        profile.apply(&stated("palette.accent", "orange"), now());
        profile.apply(
            &TasteSignal::Corrected {
                key: "hud.text_px".into(),
                from: "18".into(),
                to: "24".into(),
            },
            now(),
        );
        let text = profile.render(TASTE_PROFILE_TOKEN_BUDGET);
        let accent = text.find("palette.accent").unwrap();
        let hud = text.find("hud.text_px").unwrap();
        let lighting = text.find("lighting.preset").unwrap();
        assert!(accent < hud && hud < lighting, "{text}");
        assert!(text.contains("- avoid hud.text_px: 18"));
        let tight = profile.render(12);
        assert!(tight.contains("more"));
        assert!(estimate_text_tokens(&tight) <= 12 + 12, "{tight}");
        assert!(TasteProfile::new().render(100).is_empty());
    }

    #[test]
    fn a_project_profile_overrides_the_user_profile_key_by_key() {
        let mut user = TasteProfile::new();
        user.apply(&stated("palette.temperature", "warm"), now());
        user.apply(&stated("type.body", "Nunito"), now());
        let mut project = TasteProfile::new();
        project.apply(&stated("palette.temperature", "cool"), now());
        project.apply(
            &TasteSignal::Corrected {
                key: "style.pack".into(),
                from: "flat-vector".into(),
                to: "pixel-16".into(),
            },
            now(),
        );
        let merged = project.merged_over(&user);
        assert_eq!(merged.pin("palette.temperature").unwrap().value, "cool");
        assert_eq!(merged.pin("type.body").unwrap().value, "Nunito");
        assert!(merged.is_avoided("style.pack", "flat-vector"));
        merged.validate().unwrap();
    }

    #[test]
    fn signals_round_trip_as_json() {
        let signal = TasteSignal::Corrected {
            key: "k".into(),
            from: "a".into(),
            to: "b".into(),
        };
        let json = serde_json::to_string(&signal).unwrap();
        assert!(json.contains("\"kind\":\"corrected\""));
        let back: TasteSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(back, signal);
        let mut profile = TasteProfile::new();
        profile.apply(&signal, now());
        let json = serde_json::to_string(&profile).unwrap();
        let back: TasteProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, profile);
    }

    // ── lessons ──────────────────────────────────────────────────────────────────────

    fn draft(rule: &str, evidence: &[&str]) -> LessonDraft {
        LessonDraft {
            domain: "game-ui".to_owned(),
            trigger_tags: vec!["hud".to_owned(), "text".to_owned()],
            rule: rule.to_owned(),
            evidence: evidence.iter().map(|e| (*e).to_owned()).collect(),
        }
    }

    #[test]
    fn a_lesson_needs_evidence_a_rule_and_tags() {
        let mut book = LessonBook::new();
        assert!(matches!(
            book.propose(draft("HUD text is 24 px.", &["ep1"]), now())
                .unwrap_err(),
            LessonError::NotEnoughEvidence { have: 1, need: 2 }
        ));
        assert!(matches!(
            book.propose(draft("HUD text is 24 px.", &["ep1", "ep1"]), now())
                .unwrap_err(),
            LessonError::NotEnoughEvidence { .. }
        ));
        assert!(matches!(
            book.propose(draft("   ", &["ep1", "ep2"]), now())
                .unwrap_err(),
            LessonError::EmptyRule
        ));
        let long = "x".repeat(DESIGN_LESSON_MAX_RULE_BYTES + 1);
        assert!(matches!(
            book.propose(draft(&long, &["ep1", "ep2"]), now())
                .unwrap_err(),
            LessonError::RuleTooLong { .. }
        ));
        let mut no_tags = draft("HUD text is 24 px.", &["ep1", "ep2"]);
        no_tags.trigger_tags.clear();
        assert!(matches!(
            book.propose(no_tags, now()).unwrap_err(),
            LessonError::NoTriggerTags
        ));
        assert!(book
            .propose(draft("HUD text is 24 px.", &["ep1", "ep2"]), now())
            .is_ok());
    }

    #[test]
    fn proposed_never_renders_and_approval_is_the_users() {
        let mut book = LessonBook::new();
        let id = book
            .propose(draft("HUD text is 24 px at 1080p.", &["ep1", "ep2"]), now())
            .unwrap();
        let tags = vec!["hud".to_owned()];
        assert!(book.matching(&tags).is_empty());
        assert!(book.render(&tags, DESIGN_LESSON_TOKEN_BUDGET).is_empty());
        assert_eq!(book.proposed().len(), 1);
        book.approve(&id, now()).unwrap();
        assert_eq!(book.matching(&tags).len(), 1);
        assert!(book
            .render(&tags, DESIGN_LESSON_TOKEN_BUDGET)
            .contains("24 px"));
        assert!(book.matching(&["lighting".to_owned()]).is_empty());
        assert!(matches!(
            book.approve(&id, now()).unwrap_err(),
            LessonError::NotProposed { .. }
        ));
        book.record(&id, true).unwrap();
        book.record(&id, false).unwrap();
        assert_eq!((book.lessons[0].hits, book.lessons[0].misses), (1, 1));
    }

    #[test]
    fn a_rejected_lesson_is_not_re_proposed() {
        let mut book = LessonBook::new();
        let id = book
            .propose(draft("Never use purple.", &["ep1", "ep2"]), now())
            .unwrap();
        book.reject(&id, now()).unwrap();
        assert!(book.matching(&["hud".to_owned()]).is_empty());
        let again = book.propose(draft("never   use PURPLE.", &["ep3", "ep4"]), now());
        assert!(matches!(
            again.unwrap_err(),
            LessonError::PreviouslyRejected { .. }
        ));
        let dup_id = book
            .propose(draft("Use a plate under HUD text.", &["ep5", "ep6"]), now())
            .unwrap();
        assert!(matches!(
            book.propose(draft("Use a plate under HUD text.", &["ep7", "ep8"]), now())
                .unwrap_err(),
            LessonError::Duplicate { .. }
        ));
        assert_eq!(
            dup_id,
            LessonBook::id_for("game-ui", "Use a plate under HUD text.")
        );
    }

    #[test]
    fn approved_lessons_are_capped() {
        let mut book = LessonBook::new();
        for i in 0..DESIGN_LESSONS_MAX_APPROVED {
            let id = book
                .propose(draft(&format!("Rule number {i}."), &["a", "b"]), now())
                .unwrap();
            book.approve(&id, now()).unwrap();
        }
        let id = book
            .propose(draft("One more.", &["a", "b"]), now())
            .unwrap();
        assert!(matches!(
            book.approve(&id, now()).unwrap_err(),
            LessonError::TooManyApproved { .. }
        ));
        let text = book.render(&["hud".to_owned()], DESIGN_LESSON_TOKEN_BUDGET);
        assert!(estimate_text_tokens(&text) <= DESIGN_LESSON_TOKEN_BUDGET + 16);
        assert!(text.contains("more"));
    }

    #[test]
    fn an_episode_is_a_projection_not_a_transcript() {
        let mut episode = DesignEpisode {
            format: DESIGN_EPISODE_FORMAT.to_owned(),
            id: "ep_01".to_owned(),
            surface: DesignSurface::GameUi,
            plan_hash: "blake3:abc".to_owned(),
            sections: vec!["game-ui/hud#budget".to_owned()],
            evidence: vec!["probe_1".to_owned(), "shot_1".to_owned()],
            critique_total: Some(24),
            reaction: EpisodeReaction::Corrected,
            note: "hud.text_px 18 → 24".to_owned(),
            at: now(),
        };
        episode.validate().unwrap();
        episode.note = "the whole conversation ".repeat(40);
        assert!(episode.validate().is_err());
        episode.note = String::new();
        episode.critique_total = Some(31);
        assert!(episode.validate().is_err());
    }
}
