# BHIPPI ADE — TOKEN ENGINE / CONTEXT COMPILER
## Master Implementation Checklist

> **Purpose:** Build a production-grade context and orchestration system for an AI-native ADE with an integrated 3D game engine.
>
> **Primary goal:** Reduce unnecessary model-visible and uncached tokens while improving coding quality, orchestration reliability, and game-engine awareness.
>
> **Execution rule:** This file is a **checkable implementation plan**. The implementing AI must work through it in order, update checkboxes as work is completed, and **must not mark a task complete until the stated verification has passed**.

---

# 0. AI EXECUTION CONTRACT — READ BEFORE WRITING CODE

The AI implementing this plan must follow these rules:

- [ ] Read this entire file before changing architecture.
- [ ] Inspect the existing repository before deciding where new modules belong.
- [ ] Reuse existing systems where possible; do not duplicate functionality.
- [ ] Preserve existing user-facing behavior unless this plan explicitly changes it.
- [ ] Prefer deterministic software over LLM calls whenever deterministic logic can solve the task.
- [ ] Never use a model call merely to discover information the IDE/engine can already know programmatically.
- [ ] Never copy an entire agent/chat context into another agent by default.
- [ ] Never inject the entire repository into a model prompt.
- [ ] Never inject all tool/MCP definitions into every model call.
- [ ] Never keep unbounded terminal logs, tool output, screenshots, or stale file contents in working context.
- [ ] Every new subsystem must include tests or a deterministic verification method.
- [ ] Every checkbox marked `[x]` must correspond to code that exists and has been verified.
- [ ] If verification fails, leave the item unchecked and fix it before moving forward.
- [ ] If an architectural assumption in this document conflicts with the existing codebase, document the conflict under **Implementation Notes** and adapt the design without weakening the goal.
- [ ] Do not rewrite unrelated parts of the application.
- [ ] Avoid large refactors unless the current architecture prevents this system from being implemented correctly.
- [ ] Use feature flags for risky migrations where appropriate.
- [ ] Maintain backwards compatibility with existing projects where practical.
- [ ] Add telemetry locally for token/context diagnostics, but never leak user source code or secrets.
- [ ] Secrets, API keys, credentials, and private environment variables must never enter the Project Brain, embeddings, logs, telemetry, or LLM prompt unless explicitly required for a user-requested operation.
- [ ] When the full plan is complete, run the final validation checklist at the bottom of this file.

### Completion rule

A section is complete only when:

1. Its implementation exists.
2. Its tests pass.
3. Its acceptance criteria pass.
4. Its observability/diagnostics are available.
5. Its checkbox is marked `[x]`.

---

# 1. TARGET ARCHITECTURE

Bhippi should not use chat history as the main project memory.

The intended architecture is:

```text
                         USER
                           │
                           ▼
                    INTENT ROUTER
                           │
                           ▼
                  CONTEXT COMPILER
                           │
          ┌────────────────┼────────────────┐
          │                │                │
          ▼                ▼                ▼
     PROJECT BRAIN     WORLD BRAIN      TASK BRAIN
          │                │                │
          │                │                │
     Code / AST /      Scene / ECS /    Tasks /
     LSP / Git /       Assets / Runtime Decisions /
     Diagnostics       / Profiler        Findings
          │                │                │
          └────────────────┼────────────────┘
                           │
                           ▼
                    TOKEN GOVERNOR
                           │
                           ▼
                     MODEL ROUTER
                ┌──────────┼──────────┐
                ▼          ▼          ▼
              Claude      Codex     Others
                │          │          │
                └──────────┼──────────┘
                           ▼
                     ARTIFACT STORE
                           │
                           ▼
                       BLACKBOARD
                           │
                           ▼
                     OTHER AGENTS
```

Core principle:

> **Information has one canonical copy. Agents exchange references, deltas, structured capsules, and requested fragments—not full conversational histories.**

---

# 2. SUCCESS METRICS

Create measurable targets before implementation.

## Token targets

- [ ] Track total visible input tokens per model call.
- [ ] Track uncached/new input tokens per model call.
- [ ] Track cached input tokens separately where provider APIs expose them.
- [ ] Track output tokens.
- [ ] Track tool-definition tokens.
- [ ] Track code/file-context tokens.
- [ ] Track inter-agent handoff tokens.
- [ ] Track terminal/log tokens.
- [ ] Track conversation-history tokens.
- [ ] Track compaction savings.
- [ ] Track provider cost estimate per call.

### Initial performance targets

These are targets, not absolute guarantees:

- [ ] Routine single-file/symbol coding turn: target **800–3,000 new input tokens**.
- [ ] Simple agent-to-agent handoff: target **100–400 tokens**.
- [ ] Normal agent-to-agent handoff with code context: target **300–1,000 tokens**.
- [ ] Repository overview context: target **500–1,500 tokens**.
- [ ] Tool definitions normally active in a request: target **<1,500 tokens**.
- [ ] Raw terminal output included directly in prompts: target **near zero**.
- [ ] Full-file reads: uncommon and only when symbol/snippet retrieval is insufficient.
- [ ] Full prior-agent transcript forwarding: **zero by default**.

### Quality targets

- [ ] Retrieval must preserve or improve task success compared with the current system.
- [ ] Agent must be able to request more context when the initial capsule is insufficient.
- [ ] Token optimization must never silently hide relevant compiler errors, test failures, current diffs, or user constraints.
- [ ] Changes must remain attributable to the agent/task that made them.

---

# 3. CREATE THE BHIPPI PROJECT BRAIN

The Project Brain is persistent structured project state.

It is **not** a giant prompt and **not** a single text summary.

## 3.1 Core storage model

- [ ] Create a versioned Project Brain schema.
- [ ] Give every stored object a stable ID.
- [ ] Include `project_id`.
- [ ] Include `source_revision`.
- [ ] Include `created_at`.
- [ ] Include `updated_at`.
- [ ] Include `content_hash`.
- [ ] Include `source_of_truth`.
- [ ] Include optional `confidence`.
- [ ] Include optional `stale` state.
- [ ] Include optional `supersedes` reference.
- [ ] Support migrations between schema versions.

Suggested object types:

```text
project
module
file
symbol
type
class
function
method
variable
import
export
call_edge
reference_edge
test
diagnostic
git_commit
git_diff
scene
entity
component
prefab
material
shader
texture
mesh
animation
audio
collider
physics_layer
asset_dependency
runtime_event
profiler_sample
task
finding
decision
constraint
artifact
patch
test_result
agent_capsule
```

### Verification

- [ ] Restart the application and confirm Project Brain state persists.
- [ ] Modify a project file and confirm only affected data becomes stale.
- [ ] Confirm secrets are excluded.

---

# 4. STRUCTURAL CODE INDEX

Do not rely only on embeddings.

Use deterministic language structure wherever possible.

## 4.1 Parsing

- [ ] Add Tree-sitter or equivalent parsing for supported languages.
- [ ] Add LSP integration where available.
- [ ] Extract files.
- [ ] Extract symbols.
- [ ] Extract classes.
- [ ] Extract methods/functions.
- [ ] Extract imports/exports.
- [ ] Extract type relationships.
- [ ] Extract references.
- [ ] Extract caller/callee relationships.
- [ ] Extract inheritance.
- [ ] Extract interface implementations.
- [ ] Extract test relationships.
- [ ] Extract symbol source ranges.
- [ ] Store normalized signatures.
- [ ] Store symbol-level hashes.

## 4.2 Symbol IDs

Every symbol should have a stable semantic identity where possible.

Example:

```text
sym://project/src/player/PlayerController.ts#PlayerController.jump
```

- [ ] Symbol IDs survive unrelated line changes.
- [ ] Renames are recognized as renames when possible.
- [ ] Deleted symbols are tombstoned/versioned rather than silently lost from active task references.

## 4.3 Query API

Implement deterministic queries such as:

```text
code.find_symbol(name)
code.get_symbol(id)
code.get_callers(id)
code.get_callees(id)
code.get_references(id)
code.get_dependencies(id)
code.get_dependents(id)
code.get_tests(id)
code.get_range(id)
code.get_changed_symbols(since_revision)
```

- [ ] These queries work without an LLM call.
- [ ] Results are token-aware and support compact/full representations.

---

# 5. SEMANTIC INDEX

Use semantic search as one retrieval signal, not as the entire brain.

- [x] Chunk content primarily by symbol/module, not arbitrary fixed token windows.
- [x] Store embeddings for useful symbol/module summaries.
- [ ] Exclude generated build output.
- [ ] Exclude dependencies/vendor directories by default.
- [ ] Exclude secrets.
- [x] Re-embed only changed chunks.
- [x] Store embedding model/version metadata.
- [x] Support exact search plus semantic search.
- [x] Support hybrid scoring.
- [ ] Support recency weighting.
- [ ] Support currently-open/currently-edited weighting.
- [ ] Support active-task relevance weighting.

> B5 implemented: deterministic, dependency-free token-hash embeddings (`bhippi_providers::embedding`,
> model `bhippi-token-hash-v1`). Stored per symbol in `brain_symbols` (blob + dim + model) with a
> per-project `brain_embedding_state` table. `ProjectBrain::{search, semantic_search}` do exact-first
> hybrid ranking. Remaining open items (exclusions, recency/open-file/active-task weights) are
> enrichment and still to do.

Suggested ranking:

```text
score =
    exact_symbol_match
  + graph_relevance
  + semantic_similarity
  + current_diff_bonus
  + diagnostic_bonus
  + recency_bonus
  + active_task_bonus
  - stale_penalty
```

### Verification

- [x] Query "where is player movement handled?" and ensure relevant movement symbols rank above unrelated files.
- [x] Query an exact function name and ensure exact/structural lookup beats vector similarity.

---

# 6. MODULE KNOWLEDGE CARDS

Precompute compact module cards.

Example:

```yaml
module: combat
purpose: damage, health, armor, knockback
entry_points:
  - CombatSystem.process
public_symbols:
  - applyDamage
  - calculateArmor
events:
  - DamageEvent
  - DeathEvent
depends_on:
  - physics
  - inventory
invariants:
  - health >= 0
tests:
  - tests/combat/*
```

- [x] Generate module cards deterministically where possible.
- [ ] Allow a small AI-generated description only when deterministic metadata is insufficient.
- [x] Store descriptions separately from hard facts.
- [x] Mark generated claims with provenance.
- [x] Keep typical module cards compact.
- [x] Update cards incrementally.

Target:

- [x] Typical module card fits within roughly **50–200 tokens**.

> B8 implemented: `ProjectBrain::module_card` (and `project_module_cards`) build compact,
> deterministic per-file module cards from the structural index — `entry_points` (top-level
> fns) and `public_symbols` (top-level items, methods excluded), stored in `brain_module_cards`
> keyed by path-sans-extension. A `description` column exists (with `description_origin`
> provenance) but stays `NULL` — deterministic-only for now; the optional AI-generated
> description remains TODO. Cards are stored with a `card_revision` and recomputed only when
> the file's symbols change (incremental). Token estimate target checked via
> `module_card_token_estimate`.

---

# 7. WORLD BRAIN — 3D ENGINE KNOWLEDGE GRAPH

This is a major Bhippi differentiator.

The AI should not need to reverse-engineer serialized engine files to understand the world.

## 7.1 Scene graph

- [x] Index scenes. — ADR-0024: `brain_scenes` table, `WorldBrain::index_scene_document` (SEC. 7.1, complete 2026-09-01)
- [x] Index entities. — ADR-0024: `brain_entities` table with stable ULID keys, parent_id FK, component_names_json + component_json (complete 2026-09-01)
- [x] Index entity hierarchy. — ADR-0024: stable `scene:/root/.../name#ULID` addresses via `scene_paths` + `hierarchy` projection re-persisted to rows (complete 2026-09-01)
- [x] Index components. — ADR-0024: deterministic `BTreeMap<String, Value>` serialised as `component_json` per entity; component names materialised in `component_names_json` (complete 2026-09-01)
- [ ] Index transforms.
- [ ] Index prefabs.
- [ ] Index entity ↔ prefab relationships.
- [ ] Index script attachments.
- [ ] Index UI hierarchy.
- [ ] Index cameras.
- [ ] Index lights.
- [ ] Index input bindings.

## 7.2 Asset graph

- [x] Index materials. — ADR-0025: `brain_assets` row carries `kind` (`material`), `license`, `hash`, reverse usage (complete 2026-09-01)
- [x] Index shaders. — ADR-0025: `kind = shader` (complete 2026-09-01)
- [x] Index textures. — ADR-0025: `kind = texture` (complete 2026-09-01)
- [x] Index meshes. — ADR-0025: `kind = mesh` (complete 2026-09-01)
- [x] Index animations. — ADR-0025: `kind = animation` (complete 2026-09-01)
- [x] Index skeletons. — ADR-0025: new `AssetKind::Skeleton` + `kind = skeleton` (complete 2026-09-01)
- [x] Index audio. — ADR-0025: `kind = audio` (complete 2026-09-01)
- [x] Index asset dependencies. — ADR-0025: scene↔asset cross-reference materialised as `used_by_scenes_json` (complete 2026-09-01)
- [x] Index reverse usage: "what uses this asset?" — ADR-0025: `WorldBrain::asset_reverse_usage` resolves scene ids to names (complete 2026-09-01)

## 7.3 Physics graph

- [x] Index rigid bodies. *(complete 2026-09-01, ADR-0026)*
- [x] Index colliders. *(complete 2026-09-01, ADR-0026)*
- [ ] Blocked: collision layers — no engine data model yet (physics backend Avian, ENG-053 / P5, ADR-0020).
- [ ] Blocked: collision matrix — no engine data model yet (physics backend Avian, ENG-053 / P5, ADR-0020).
- [ ] Blocked: joints/constraints — no engine data model yet (physics backend Avian, ENG-053 / P5, ADR-0020).
- [ ] Blocked: navigation information — no engine data model yet (physics backend Avian, ENG-053 / P5, ADR-0020).

> Note (2026-09-01, ADR-0026): bodies + colliders are derived from the per-entity
> `RigidBody` / `Collider` / `CharacterController` components already snapshotted by
> SEC 7.1 and persisted into `brain_physics_bodies` (migration 0010) with WorldBrain
> queries + IPC + Brain-panel physics section. Items 3-6 (layers, matrix, joints,
> navigation) are blocked on the physics backend because the engine has no such data.

## 7.4 Engine query API

> Note (2026-09-01, ADR-0027): implemented as a pure `bhippi-engine` read-only facade
> (`SceneQueries`, `crates/bhippi-engine/src/api.rs`) covering all 13 queries below —
> deterministic (authoring order / `BTreeMap` order), with `compact` and `deep` expansion
> modes. Scope per owner: API + tests only (no IPC / World Brain / UI), as follow-ups.

Implement:

```text
scene.get(id)
scene.get_entity(id)
scene.find_entities(query)
scene.get_components(entity_id)
scene.get_children(entity_id)
scene.get_parent(entity_id)
scene.get_scripts(entity_id)
scene.get_asset_dependencies(asset_id)
scene.get_asset_users(asset_id)
scene.get_material_graph(material_id)
scene.get_shader(shader_id)
scene.get_animation_graph(entity_id)
scene.get_physics(entity_id)
```

- [x] All above are deterministic.
- [x] Each query supports a compact representation.
- [x] Each query supports a deeper expansion mode.

---

# 8. RUNTIME BRAIN

Index what happens while the game/app is running.

## 8.1 Runtime events

- [ ] Capture engine errors.
- [ ] Capture warnings.
- [ ] Capture script exceptions.
- [ ] Capture entity spawn/despawn.
- [ ] Capture component changes relevant to debugging.
- [ ] Capture collisions.
- [ ] Capture animation transitions.
- [ ] Capture asset loads.
- [ ] Capture shader compilation.
- [ ] Capture selected networking events if applicable.
- [ ] Capture test/runtime assertion failures.

## 8.2 Performance

- [ ] Capture frame time.
- [ ] Capture CPU time.
- [ ] Capture GPU time where available.
- [ ] Capture memory spikes.
- [ ] Capture allocation spikes.
- [ ] Capture draw calls.
- [ ] Capture shader compilation stalls.
- [ ] Capture scene load cost.
- [ ] Capture entity count changes.

## 8.3 Runtime summarizer

Do not send raw traces by default.

Create a deterministic compact summary:

```text
Trigger: Enter Village
Frame time: 11ms -> 67ms
New entities: +427
Memory: +612MB

Top suspects:
Trees.prefab      17ms
Water.shader      13ms
NPCSpawner         9ms
VillageProps       8ms
```

- [ ] Raw data stays available by reference.
- [ ] Model receives summary first.
- [ ] Model can request a specific raw range later.

---

# 9. CHANGE INDEX / INCREMENTAL INDEXING

The current "Index DB" button should become a manual repair/rebuild action, not the normal indexing workflow.

## 9.1 File watcher

- [x] Watch files incrementally.
- [x] Hash changed files.
- [x] Ignore unchanged files.
- [x] Reparse only changed files.
- [ ] Recompute only affected graph edges.
- [x] Re-embed only changed symbols/modules.
- [ ] Mark impacted module cards stale.
- [ ] Refresh cards asynchronously.
- [x] Track index revision.

> B6 implemented: `ProjectBrain::reindex_tree` is an incremental, scan-based watcher —
> walks the tree (skipping hidden entries and the `default_excludes` build/vendor dirs),
> re-parses and re-embeds only files whose content hash changed, ignores unchanged files,
> and marks tracked files that vanished as stale (reconciling their symbols away). The
> project source revision is bumped once when the tree changed. Graph-edge recompute and
> module-card invalidation remain TODO (graph and card phases are not yet built).

## 9.2 Engine watcher

- [ ] Track scene modifications.
- [ ] Track component modifications.
- [ ] Track asset imports/reimports.
- [ ] Track prefab updates.
- [ ] Track material/shader changes.
- [ ] Update only affected world graph nodes.

## 9.3 Manual command

Keep a user-facing action:

```text
Rebuild Project Brain
```

- [x] Manual rebuild validates and repairs all indices.
- [x] UI shows indexing progress.

> B6/B8 app+UI wiring: `bhippi-app` owns a `bhippi_db::Database` at `~/.bhippi/brain.db`
> and exposes `rebuild_project_brain` (drives `ProjectBrain::reindex_tree` returning an
> `IndexReport`), `project_brain_status`, `list_project_module_cards`,
> `get_project_module_card`, and `search_project_symbols`. The UI adds a **Brain** action
> in the title bar opening the `ProjectBrainPanel`: it shows index status (symbols,
> modules, revision, embedding model), a "Rebuild Project Brain" button that reports the
> re-index result, a symbol search, and the module knowledge cards. The manual
> "Index DB"-style repair path is now usable end-to-end.
- [ ] UI shows index revision.
- [ ] UI shows stale/healthy status.

---

# 10. TASK BRAIN / SHARED BLACKBOARD

Agents should collaborate through shared state.

Do not make chats the canonical source of truth.

## 10.1 Task schema

Example:

```yaml
id: T842
goal: Fix excessive enemy knockback
status: implementing
owner: codex-2
depends_on:
  - T839
relevant_symbols:
  - S381
  - S442
relevant_entities:
  - E91
findings:
  - F91
decisions:
  - D31
patches:
  - P17
tests:
  - R81
blockers: []
revision: 14
```

- [ ] Create task IDs.
- [ ] Track status.
- [ ] Track owner.
- [ ] Track dependencies.
- [ ] Track relevant code/world refs.
- [ ] Track findings.
- [ ] Track decisions.
- [ ] Track patches.
- [ ] Track test results.
- [ ] Track blockers.
- [ ] Track revision history.

Recommended task states:

```text
planned
ready
running
blocked
review
done
failed
cancelled
```

---

# 11. AGENT CONTEXT ISOLATION

Each agent gets a temporary working context.

Important information moves into the blackboard/project brain.

- [ ] Claude sessions do not automatically inherit Codex chat history.
- [ ] Codex sessions do not automatically inherit Claude chat history.
- [ ] New agent receives a task capsule plus references.
- [ ] Agent can explicitly request referenced context.
- [ ] Agent can write findings back to shared state.
- [ ] Agent can publish patches.
- [ ] Agent can publish decisions.
- [ ] Agent can publish blockers.
- [ ] Agent can publish tests/results.
- [ ] Agent local history can be compacted independently.

---

# 12. AGENT CAPSULE FORMAT

Every agent handoff should create a compact structured capsule.

Example:

```yaml
capsule_id: C912
task: T842
from: claude-1
to: codex-2
state: ready_for_implementation

goal: Fix excessive enemy knockback

facts:
  - ref: F91
    text: Damage impulse is applied twice.

relevant:
  symbols:
    - S381
    - S442
  entities:
    - E91

constraints:
  - D31

artifacts:
  - patch: P17
  - test_result: R81

blockers: []

recommended_next_action:
  Implement clamp inside DamageReceiver and rerun knockback tests.
```

## Rules

- [ ] No full source file inside capsule unless unavoidable.
- [ ] No full transcript.
- [ ] No raw terminal log.
- [ ] No hidden reasoning/chain-of-thought.
- [ ] Store facts as refs + concise descriptions.
- [ ] Include only task-relevant decisions.
- [ ] Include only relevant code/world IDs.
- [ ] Keep capsule deterministic where possible.

Targets:

- [ ] Simple capsule: **100–400 tokens**.
- [ ] Normal capsule: **300–1,000 tokens**.
- [ ] Larger capsule must justify why additional context is needed.

---

# 13. REFERENCE-BASED AGENT COMMUNICATION

Replace copy/paste with references.

Instead of:

```text
Claude -> Codex:
[18,000 token history]
```

Use:

```text
TASK:T842
FINDING:F91
PATCH:P17
TEST:R81
SYMBOL:S381
ENTITY:E91
```

Implement:

```text
brain.get(ref)
brain.get_many(refs)
brain.expand(ref, depth)
brain.get_delta(old_revision, new_revision)
```

- [ ] Every important artifact is referenceable.
- [ ] References are stable across agent sessions.
- [ ] Permission boundaries are enforced.
- [ ] Ref resolution is logged for debugging.
- [ ] Ref expansion is token-budget aware.

---

# 14. DELTA COMMUNICATION

Agents should receive changes since the revision they already know.

Example:

```text
Known snapshot: 551
Current snapshot: 552

Delta:
- S14 changed
- S91 changed
- D31 added
```

Implement deltas for:

- [ ] Project Brain.
- [ ] Code graph.
- [ ] Git diff.
- [ ] Scene graph.
- [ ] Asset graph.
- [ ] Runtime diagnostics.
- [ ] Task brain.
- [ ] Agent findings.
- [ ] Decisions.
- [ ] Test results.

Target:

- [ ] If an agent already knows a revision, do not resend unchanged data.

---

# 15. CONTEXT COMPILER

This is the central system.

The Context Compiler takes:

```text
user request
+ active project
+ active task
+ active selection
+ current runtime state
+ provider/model
```

and produces the smallest useful context package.

## 15.1 Inputs

- [ ] User request.
- [ ] Current file/symbol.
- [ ] Current editor selection.
- [ ] Current scene/entity selection.
- [ ] Current task.
- [ ] Current Git diff.
- [ ] Current diagnostics.
- [ ] Current runtime issue.
- [ ] Relevant recent decisions.
- [ ] Agent-known revision.
- [ ] Provider cache state.
- [ ] Available token budget.

## 15.2 Retrieval stages

Run multiple retrieval strategies:

- [ ] Exact symbol match.
- [ ] AST/LSP graph lookup.
- [ ] Dependency graph lookup.
- [ ] Semantic retrieval.
- [ ] Git/changed-code relevance.
- [ ] Diagnostic relevance.
- [ ] Task relevance.
- [ ] Scene/entity relevance.
- [ ] Runtime relevance.
- [ ] Recency weighting.

## 15.3 Progressive context levels

Use:

```text
L0 = project/task identity
L1 = module cards / graph facts
L2 = symbol signatures / relationships
L3 = exact relevant code ranges
L4 = full files / large artifacts
```

Rules:

- [ ] Start with the lowest sufficient level.
- [ ] L4 is never the default.
- [ ] Model can request deeper expansion.
- [ ] Expansion must be targeted by ref/symbol/range.

---

# 16. CONTEXT CAPSULE

The model should receive a compiled context capsule.

Example:

```yaml
task:
  id: T842
  goal: Fix excessive enemy knockback

project:
  revision: 552
  module: combat

known_facts:
  - ref: F91
    value: Damage impulse is applied twice.

relevant_symbols:
  - id: S381
    signature: DamageReceiver.applyDamage(event: DamageEvent): void
  - id: S442
    signature: CharacterMotor.applyImpulse(v: Vec3): void

code_ranges:
  - ref: S381
    range: 41-78

runtime:
  - ref: E55
    summary: velocity spike follows DamageEvent twice in same frame

constraints:
  - ref: D31
    summary: normal jump behavior must remain unchanged

current_diff:
  ref: P17
```

- [ ] Capsule has explicit token estimate before send.
- [ ] Capsule includes provenance.
- [ ] Capsule avoids duplicates.
- [ ] Capsule avoids stale facts.
- [ ] Capsule links larger artifacts instead of embedding them.

---

# 17. TOKEN GOVERNOR

The Token Governor enforces budgets.

## 17.1 Budget categories

Track:

```text
system
tools
project context
task context
conversation
code
world/engine
runtime
diagnostics
git diff
tool results
handoff
reserved response
```

- [ ] Assign soft budgets.
- [ ] Assign hard ceilings.
- [ ] Reserve output/reasoning headroom.
- [ ] Trim lowest-value content first.
- [ ] Never trim required user constraints.
- [ ] Never silently drop active error/test state.

Suggested initial normal request budget:

```text
System/cache            stable
Core tools              stable
Task                    100-300
Project/module          100-500
Relevant symbols        200-800
Code                     300-2,000
Runtime/diagnostics     100-800
Recent diff             100-800
```

---

# 18. CONTEXT DEDUPLICATION

Before model invocation:

- [ ] Deduplicate repeated code ranges.
- [ ] Deduplicate repeated module descriptions.
- [ ] Deduplicate repeated task descriptions.
- [ ] Deduplicate repeated decisions.
- [ ] Deduplicate repeated diagnostics.
- [ ] Do not include same content in both tool result and context capsule.
- [ ] Prefer canonical ref over duplicate inline text.

---

# 19. TOOL ROUTER / DYNAMIC TOOL LOADING

Do not expose all tools to every request.

## 19.1 Always-available compact core

Aim for a very small core such as:

```text
brain.search
brain.get
code.query
code.patch
scene.query
scene.patch
run
capability.search
```

- [ ] Keep core schemas short.
- [ ] Ensure tools return refs and compact summaries.
- [ ] Tools support pagination/range requests.

## 19.2 Dynamic tools

Examples:

```text
deploy.cloudflare
git.github
blender.mesh_edit
browser.navigate
database.query
```

- [ ] Load only when relevant.
- [ ] Remove when no longer required.
- [ ] Provider tool ordering remains stable for cache reuse.
- [ ] Tool router can operate locally/deterministically where possible.

---

# 20. LOCAL / ZERO-CLOUD-TOKEN FAST PATH

Do not call a large model for deterministic actions.

Examples:

```text
Move selected cube up 2m
Duplicate selected light
Set camera FOV to 80
Rename object
Open PlayerController
Run project
Stop project
Hide entity
Set material roughness
Create simple primitive
Attach known component
```

Implement:

- [ ] Intent parser for known deterministic actions.
- [ ] Current selection awareness.
- [ ] Confirmation rules for destructive operations.
- [ ] Engine command execution.
- [ ] Undo support.
- [ ] Fallback to model only if intent is ambiguous or reasoning is required.

Target:

- [ ] Common engine/editor commands use **zero cloud model tokens**.

---

# 21. TERMINAL OUTPUT COMPRESSOR

Raw terminal output must not flood prompts.

Pipeline:

```text
terminal
   ↓
local parser
   ↓
structured result
   ↓
model
```

Example:

```yaml
command: npm test
status: failed
passed: 137
failed: 3

failures:
  - file: EnemyMovement.test.ts
    line: 81
    expected: velocity.y > 0
    received: 0

raw_log_ref: LOG_881
```

- [ ] Parse test output.
- [ ] Parse compiler output.
- [ ] Parse linter output.
- [ ] Parse package-manager errors.
- [ ] Parse common stack traces.
- [ ] Keep raw log by reference.
- [ ] Support `read_log(ref, range)`.

Target:

- [ ] Default prompt gets concise failure summary, not raw log.

---

# 22. DIAGNOSTIC COMPILER

The IDE already knows many facts the model should not need to rediscover.

- [ ] Aggregate compiler errors.
- [ ] Aggregate LSP errors.
- [ ] Aggregate lint errors.
- [ ] Merge duplicates.
- [ ] Link errors to symbols.
- [ ] Link errors to current diff.
- [ ] Prioritize errors related to active task.
- [ ] Provide compact diagnostic bundle.

---

# 23. GIT-AWARE CONTEXT

Prefer diffs over resending whole changed files.

Implement:

```text
git.current_revision
git.diff
git.changed_files
git.changed_symbols
git.blame_summary
git.related_commits
```

- [ ] Include only relevant hunks.
- [ ] Link hunks to symbols.
- [ ] Track which agent authored each patch if possible.
- [ ] Allow context compiler to compare current state with agent-known revision.

---

# 24. PATCH-FIRST EDITING

Where safe, prefer structured patches over model-regenerated files.

- [ ] Model edits exact symbol/range where possible.
- [ ] Patch application checks source revision.
- [ ] Reject stale patch if source changed.
- [ ] Format after patch.
- [ ] Run diagnostics after patch.
- [ ] Run relevant tests.
- [ ] Store patch as an artifact reference.

---

# 25. PROVIDER-SPECIFIC CACHE MANAGERS

Do not use one generic request-builder for every provider.

Create adapters such as:

```text
ClaudeContextAdapter
OpenAIContextAdapter
GeminiContextAdapter
OtherProviderAdapter
```

## Requirements

- [ ] Stable system prompt ordering.
- [ ] Stable tool ordering.
- [ ] Stable project instruction ordering.
- [ ] Append changing content rather than rewriting earlier stable prefix where provider caching benefits from exact prefixes.
- [ ] Track cache-hit/cache-read usage when API exposes it.
- [ ] Keep provider-specific ephemeral cache behavior isolated from project-brain logic.
- [ ] Ensure tool definitions do not reorder nondeterministically.

---

# 26. CONVERSATION COMPACTION

Chats are useful UI but must not grow forever as raw context.

## Rules

- [ ] Keep recent turns verbatim within a small window.
- [ ] Convert older completed work into task facts, decisions, artifacts, and summaries.
- [ ] Remove obsolete tool output.
- [ ] Remove old raw logs.
- [ ] Remove stale file copies.
- [ ] Preserve explicit user constraints.
- [ ] Preserve unresolved blockers.
- [ ] Preserve decisions still affecting current work.
- [ ] Preserve artifact refs.
- [ ] Store compacted state with revision/provenance.

Do not use generic "summarize everything" as the only mechanism.

---

# 27. MEMORY POLICY

Separate types of memory.

## 27.1 Working memory

Temporary:
- current task
- current file/symbol
- latest patch
- latest diagnostics
- latest relevant conversation

## 27.2 Project memory

Persistent:
- architecture decisions
- conventions
- invariants
- known failed approaches
- user-approved constraints

## 27.3 World memory

Persistent:
- scene/entity/component relationships
- asset relationships
- runtime/config data

## Rules

- [ ] Memory is stored externally.
- [ ] Memory is retrieved on demand.
- [ ] Memory is not automatically pasted into every request.
- [ ] Old memory can expire or be superseded.
- [ ] Facts carry provenance.
- [ ] Conflicting memories are surfaced rather than silently merged.

---

# 28. DECISION STORE

Create first-class decision objects.

Example:

```yaml
id: D31
scope: combat
decision: Normal jump behavior must remain unchanged.
reason: User approved existing jump feel.
source: user
status: active
```

- [ ] Decisions can be scoped.
- [ ] Decisions can be superseded.
- [ ] Decisions can be queried by task/module.
- [ ] User decisions outrank AI assumptions.
- [ ] Only relevant decisions enter context.

---

# 29. FINDING STORE

Agents should publish concise findings.

Example:

```yaml
id: F91
task: T842
type: root_cause
fact: Damage impulse is applied twice.
evidence:
  - S381
  - E55
confidence: high
```

- [ ] Findings require evidence refs where possible.
- [ ] Findings can be invalidated.
- [ ] Findings can be superseded.
- [ ] Only relevant findings enter subsequent context.

---

# 30. ARTIFACT STORE

Large results should live outside chat context.

Artifacts may include:

```text
patch
diff
test log
screenshot
profiling capture
generated file
research output
scene snapshot
build report
```

- [ ] Store artifacts with stable references.
- [ ] Store content hash.
- [ ] Store MIME/type.
- [ ] Store owner/task.
- [ ] Store revision.
- [ ] Agents pass artifact refs.
- [ ] Context compiler embeds only a compact summary unless full artifact is requested.

---

# 31. SCREENSHOT / VIEWPORT CONTEXT

When vision is used, attach engine metadata.

Example:

```yaml
viewport: 1920x1080
scene: Village_Night
selected_entity: E381
hovered_entity: E912
visible_entities: 148
camera: MainCamera
render_mode: deferred
active_errors: 2
```

- [ ] Vision is supplementary to structured engine information.
- [ ] Do not ask the model to visually infer data Bhippi knows exactly.
- [ ] Crop/resize screenshots intelligently when only a region matters.

---

# 32. ORCHESTRATOR POLICY

Do not use many agents just because multiple providers exist.

## Decision flow

```text
Can one agent solve the task reliably?
    yes -> use one agent

Does the task have independent parallel subtasks?
    yes -> consider multiple agents

Does a second agent add verification value?
    yes -> reviewer/QA agent

Otherwise:
    do not spawn additional agents
```

- [ ] Add agent-spawn cost estimate.
- [ ] Add expected benefit score.
- [ ] Add max parallel-agent limit.
- [ ] Prefer specialist role assignment.
- [ ] Do not duplicate the same investigation across agents unless explicitly doing verification.

---

# 33. AGENT ROLES

Suggested roles:

```text
Coordinator
Implementation
Research
Engine/Scene
Debug
Test/QA
Review
```

- [ ] Each role has minimal role-specific instructions.
- [ ] Do not send all role prompts to all agents.
- [ ] Role instructions are cache-stable.
- [ ] Each agent receives only relevant tools.

---

# 34. NO CHAIN-OF-THOUGHT TRANSFER

Agent communication must transfer:

```text
facts
evidence
decisions
patches
tests
blockers
recommended next action
```

It must **not** require transferring private chain-of-thought or huge reasoning transcripts.

- [ ] Handoff schema excludes hidden reasoning.
- [ ] Reviewer judges artifact/evidence, not another agent's private reasoning text.

---

# 35. CONTEXT REQUEST PROTOCOL

Agents need a safe way to request more information.

Implement actions such as:

```text
context.expand(ref)
context.symbol(id)
context.callers(id)
context.dependencies(id)
context.code_range(id, start, end)
context.scene_entity(id)
context.asset(id)
context.runtime(ref)
context.log(ref, range)
context.task(id)
context.delta(from, to)
```

- [ ] Each expansion has a token estimate.
- [ ] Agent is shown compact results first.
- [ ] Excessively large expansions require another narrower query or explicit justification.

---

# 36. SMART ROUTING BEFORE LLM CALL

Before invoking a model, classify the request locally:

```text
editor command
engine command
symbol lookup
search/navigation
deterministic refactor
simple fix
complex coding
research
debug
orchestration
```

- [ ] Deterministic command -> no LLM.
- [ ] Lookup/navigation -> index first.
- [ ] Coding -> compile context.
- [ ] Debug -> diagnostics/runtime first.
- [ ] Research -> separate research agent/context.
- [ ] Orchestration -> task graph + capsules.

---

# 37. RESEARCH AGENT ISOLATION

Web research can consume enormous context.

- [ ] Research agent gets its own context.
- [ ] Research raw sources do not enter implementation agent context by default.
- [ ] Research result becomes structured findings + citations/refs.
- [ ] Implementation agent receives only relevant conclusions.
- [ ] Full research artifact remains retrievable by ref.

---

# 38. TEST AGENT ISOLATION

QA agent should not need full implementation conversation.

It receives:

```text
task goal
acceptance criteria
patch/diff ref
relevant files/symbols
test commands
known constraints
```

- [ ] QA can independently inspect necessary code via refs.
- [ ] QA publishes pass/fail findings.
- [ ] QA result updates task state.

---

# 39. TOKEN OBSERVABILITY UI

Add a developer-facing token panel.

Display per request:

```text
Visible context        12.8K
New input                814
Cached input            11.9K
Output                  1.4K
Tools                    620
Code                     410
Handoff                  183
Estimated cost         $0.0xx
```

- [ ] Per-chat totals.
- [ ] Per-project totals.
- [ ] Per-agent totals.
- [ ] Per-provider totals.
- [ ] Average new tokens per successful task.
- [ ] Cache hit ratio.
- [ ] Most expensive context category.
- [ ] Handoff token cost.
- [ ] Tool-schema overhead.
- [ ] Compaction savings.

---

# 40. CONTEXT DEBUGGER

Create a way for developers to inspect exactly why information was selected.

Example:

```text
S381 selected because:
+ exact task symbol match
+ changed in current diff
+ linked to runtime diagnostic E55
+ called by active module
```

- [ ] Show selected context items.
- [ ] Show ranking score.
- [ ] Show token cost.
- [ ] Show omitted items.
- [ ] Show provider cache boundaries.
- [ ] Allow copying a sanitized context manifest for debugging.

---

# 41. QUALITY GUARDRAIL

Low-token context must not become low-quality context.

Before send:

- [ ] Check all explicit user requirements are present.
- [ ] Check current task goal is present.
- [ ] Check relevant current diff is present.
- [ ] Check related errors/test failures are present.
- [ ] Check relevant decision constraints are present.
- [ ] Check content is not stale.
- [ ] Check references resolve.
- [ ] Check no secret entered context.
- [ ] Check model has a way to request more context.

---

# 42. FALLBACK POLICY

When retrieval confidence is low:

Do **not** hallucinate project structure.

Instead:

1. [ ] Broaden deterministic search.
2. [ ] Search semantic index.
3. [ ] Search dependency graph.
4. [ ] Ask model which specific missing artifact it needs.
5. [ ] Expand context incrementally.
6. [ ] Use full file only as a later fallback.

---

# 43. SECURITY / PRIVACY

- [ ] Never index `.env` values.
- [ ] Never embed secrets.
- [ ] Redact API keys from logs.
- [ ] Respect ignored/private files.
- [ ] Support project-level indexing exclusions.
- [ ] Local Project Brain by default unless user explicitly enables cloud synchronization.
- [ ] Encrypt sensitive persistent metadata where appropriate.
- [ ] Enforce agent permissions when resolving refs.
- [ ] Log destructive engine/editor actions.
- [ ] Support undo/rollback where possible.

---

# 44. FAILURE RECOVERY

- [ ] Project Brain corruption detection.
- [ ] Rebuild index.
- [ ] Rollback failed schema migration.
- [ ] Recover stale task state.
- [ ] Detect broken refs.
- [ ] Detect orphan artifacts.
- [ ] Detect agent crash while task is running.
- [ ] Reassign abandoned task.
- [ ] Preserve incomplete patch safely.
- [ ] Continue from last verified task revision, not raw conversation alone.

---

# 45. IMPLEMENTATION ORDER

The implementing AI must use this order unless the repository architecture strongly requires a dependency change.

## Phase A — Measurement first

- [x] A1. Add token/context telemetry.
- [x] A2. Record current baseline on representative tasks.
- [x] A3. Measure tool-schema overhead.
- [x] A4. Measure handoff overhead.
- [x] A5. Measure repository-context overhead.
- [x] A6. Save baseline report.

**DO NOT move to Phase B until baseline metrics exist.**

---

## Phase B — Project Brain

- [ ] B1. Create schemas/storage.
- [ ] B2. Implement stable IDs.
- [ ] B3. Implement revisions/hashes.
- [ ] B4. Add structural code indexing.
- [ ] B5. Add semantic index.
- [ ] B6. Add incremental indexing.
- [ ] B7. Add query APIs.
- [ ] B8. Add module cards.
- [ ] B9. Verify persistence and correctness.

**DO NOT move to Phase C until Project Brain queries can locate relevant symbols without an LLM.**

---

## Phase C — World Brain

- [ ] C1. Scene graph.
- [ ] C2. Entity/component graph.
- [ ] C3. Asset graph.
- [ ] C4. Physics metadata.
- [ ] C5. Runtime events.
- [ ] C6. Profiler/performance summaries.
- [ ] C7. Engine query APIs.
- [ ] C8. Incremental world updates.

**DO NOT move to Phase D until selected entities/assets can be described compactly without reading serialized files.**

---

## Phase D — Blackboard / Task Brain

- [ ] D1. Task schema.
- [ ] D2. Finding schema.
- [ ] D3. Decision schema.
- [ ] D4. Artifact store.
- [ ] D5. Agent capsule schema.
- [ ] D6. Ref resolution.
- [ ] D7. Revision/delta system.
- [ ] D8. Migrate agent handoffs away from full-chat copying.

**DO NOT move to Phase E until two agents can collaborate using only task/capsule refs.**

---

## Phase E — Context Compiler

- [ ] E1. Intent classifier.
- [ ] E2. Hybrid retrieval.
- [ ] E3. Progressive disclosure.
- [ ] E4. Context capsule builder.
- [ ] E5. Deduplication.
- [ ] E6. Token Governor.
- [ ] E7. Context quality guardrail.
- [ ] E8. Context expansion protocol.

---

## Phase F — Tool optimization

- [ ] F1. Minimal core tools.
- [ ] F2. Dynamic capability loading.
- [ ] F3. Local deterministic commands.
- [ ] F4. Terminal output compressor.
- [ ] F5. Diagnostic compiler.
- [ ] F6. Git-aware context.
- [ ] F7. Patch-first editing.

---

## Phase G — Provider optimization

- [ ] G1. Claude cache-aware adapter.
- [ ] G2. OpenAI/Codex cache-aware adapter.
- [ ] G3. Gemini cache-aware adapter.
- [ ] G4. Stable tool ordering.
- [ ] G5. Cache usage telemetry.
- [ ] G6. Conversation compaction.

---

## Phase H — Orchestration optimization

- [ ] H1. Agent spawn policy.
- [ ] H2. Context-isolated specialist agents.
- [ ] H3. Research isolation.
- [ ] H4. QA/review isolation.
- [ ] H5. Delta handoffs.
- [ ] H6. Eliminate full transcript forwarding.
- [ ] H7. Inter-agent token dashboard.

---

# 46. ACCEPTANCE TEST SCENARIOS

Run these before declaring the system production-ready.

## Scenario 1 — Simple symbol edit

Prompt:

```text
Change player jump force from 14 to 11.
```

Expected:

- [ ] Bhippi resolves exact symbol/config without scanning full repo.
- [ ] No second agent.
- [ ] No full file unless necessary.
- [ ] Patch is minimal.
- [ ] Relevant tests/diagnostics run.
- [ ] New input context stays within target range.

---

## Scenario 2 — Cross-file bug

Prompt:

```text
Enemy receives knockback twice after damage.
```

Expected:

- [ ] Retrieval uses callers/event graph.
- [ ] Only relevant symbols/code ranges supplied.
- [ ] Runtime diagnostic linked if available.
- [ ] Fix is tested.
- [ ] Full repository is never sent.

---

## Scenario 3 — 3D scene issue

Prompt:

```text
Why does this enemy fall through the floor?
```

Expected:

- [ ] Current selected entity used automatically.
- [ ] Collider/rigidbody/layer info comes from World Brain.
- [ ] Runtime physics evidence included.
- [ ] Agent does not parse massive scene serialization unless required.
- [ ] Relevant script symbols retrieved only if needed.

---

## Scenario 4 — Performance issue

Prompt:

```text
Why does the game lag when entering the village?
```

Expected:

- [ ] Runtime profiler summary first.
- [ ] Relevant scene/assets linked.
- [ ] Agent receives top performance offenders.
- [ ] Raw profiler capture remains referenced.
- [ ] Agent can expand only needed traces.

---

## Scenario 5 — Multi-agent handoff

Claude investigates.
Codex implements.
QA reviews.

Expected:

- [ ] Claude publishes finding capsule.
- [ ] Codex receives capsule, not Claude transcript.
- [ ] Codex publishes patch.
- [ ] QA receives task + patch + constraints, not implementation transcript.
- [ ] Handoff token cost is recorded.
- [ ] Target handoff token budget is respected where feasible.

---

## Scenario 6 — Large terminal log

- [ ] Produce >10,000-token failing test/build log.
- [ ] Raw log is stored by ref.
- [ ] Model receives concise summary.
- [ ] Model can request specific relevant raw lines.
- [ ] Raw log is not retained indefinitely in chat context.

---

## Scenario 7 — Cache stability

Run several turns without changing core instructions.

Expected:

- [ ] Stable provider prefix remains byte/order stable where required.
- [ ] Tool order stays stable.
- [ ] Cache reads/hits increase.
- [ ] Uncached/new tokens remain much lower than logical visible context.

---

# 47. BENCHMARK SUITE

Create a repeatable benchmark suite.

Include:

- [ ] 10 simple edits.
- [ ] 10 cross-file coding tasks.
- [ ] 10 debugging tasks.
- [ ] 10 engine/scene tasks.
- [ ] 5 performance tasks.
- [ ] 5 multi-agent tasks.
- [ ] 5 research + implementation tasks.

For each benchmark record:

```text
success/failure
total input tokens
new input tokens
cached tokens
output tokens
tool tokens
handoff tokens
number of model calls
number of agents
wall-clock time
estimated cost
files/symbols opened
tests passed
```

Compare:

```text
CURRENT BHIPPI
vs
NEW TOKEN ENGINE
```

- [ ] Require no regression in task quality.
- [ ] Report median token reduction.
- [ ] Report p90 token reduction.
- [ ] Report average inter-agent handoff reduction.

---

# 48. UI CHANGES

Add subtle developer-facing controls.

## Project Brain

- [ ] Brain status indicator.
- [ ] Index revision.
- [ ] Last update.
- [ ] Rebuild button.
- [ ] Stale/error indicator.

## Agent session

Optional compact indicators:

```text
New 814
Cache 11.9K
Handoff 183
```

- [ ] Detailed token breakdown only on hover/details panel.
- [ ] Keep default UI clean.

## Agent graph

- [ ] Show agents connected through task/artifact refs.
- [ ] Do not visually imply full-chat copying.
- [ ] Show current owner/status/blocker.

---

# 49. IMPORTANT ANTI-PATTERNS — NEVER DO THESE BY DEFAULT

- [ ] Never paste entire repository tree into every prompt.
- [ ] Never paste entire source files when a symbol range is enough.
- [ ] Never pass full agent transcript to another agent.
- [ ] Never keep all terminal output in long-term conversation context.
- [ ] Never expose all MCP/provider tools on every call.
- [ ] Never use embeddings as the only retrieval mechanism.
- [ ] Never rebuild full index after every file save.
- [ ] Never ask LLM to compute information the compiler/LSP/engine already knows.
- [ ] Never spawn multiple agents for trivially sequential work.
- [ ] Never use LLM summarization where a deterministic parser can produce a more accurate compact representation.
- [ ] Never optimize tokens by dropping user constraints or current error state.
- [ ] Never let a stale Project Brain fact silently override current source/runtime state.

---

# 50. DEFINITION OF DONE

The project is not complete until all are true.

## Core architecture

- [ ] Project Brain exists and is persistent.
- [ ] Structural code graph exists.
- [ ] Semantic index exists.
- [ ] World Brain exists.
- [ ] Runtime Brain exists.
- [ ] Task Brain/blackboard exists.
- [ ] Artifact references exist.
- [ ] Incremental indexing works.

## Context

- [ ] Context Compiler works.
- [ ] Token Governor works.
- [ ] Progressive disclosure works.
- [ ] Context deduplication works.
- [ ] Agents can request targeted expansion.
- [ ] Full repository is not injected by default.

## Orchestration

- [ ] Agent handoff uses capsules/refs.
- [ ] Full transcript forwarding is disabled by default.
- [ ] Delta communication works.
- [ ] Agent context isolation works.
- [ ] Spawn policy avoids unnecessary agents.

## Tools

- [ ] Minimal core tool set.
- [ ] Dynamic capability loading.
- [ ] Terminal compression.
- [ ] Diagnostic compression.
- [ ] Git diff context.
- [ ] Deterministic engine fast path.

## Providers

- [ ] Cache-aware Claude adapter.
- [ ] Cache-aware OpenAI/Codex adapter.
- [ ] Cache-aware Gemini/other adapters where relevant.
- [ ] Stable tool/instruction ordering.
- [ ] Provider token telemetry.

## Quality

- [ ] Benchmark suite passes.
- [ ] Token reduction is demonstrated.
- [ ] No meaningful quality regression.
- [ ] Existing projects still open and function.
- [ ] Security/privacy tests pass.
- [ ] Recovery/rebuild works.

---

# 51. FINAL REPORT — AI MUST FILL THIS IN

When implementation is complete, the implementing AI must replace this section with actual measured results.

```text
Implementation status:
[ ] COMPLETE
[ ] PARTIAL
[ ] BLOCKED

Baseline median new input tokens:
________________

New system median new input tokens:
________________

Reduction:
________________ %

Baseline median handoff tokens:
________________

New median handoff tokens:
________________

Handoff reduction:
________________ %

Cache hit/read ratio:
________________

Task success baseline:
________________

Task success new system:
________________

Known remaining issues:
1.
2.
3.

Files/modules added:
1.
2.
3.

Files/modules modified:
1.
2.
3.

Tests added:
1.
2.
3.

Final verification command(s):
________________
```

---

# 52. IMPLEMENTATION NOTES

The implementing AI must write important architecture deviations, migrations, blockers, or compatibility concerns here.

```text
- A1: Per-turn telemetry lives in bhippi-core::context (bhippi_core::ContextSampleStore),
  persisted to ~/.bhippi/context.json, capped at 2_000 samples (RETAINED_SAMPLES), local-only
  (INV-039): counts and metadata only, never message text or prompts. Categories mirror this
  plan's ledger (system/workspace/project_rules/skills/computer_use/engine/handoff/conversation/
  task_directives/reserved_response/tool_schemas). Estimates use the same bytes/4 heuristic as
  bhippi_app's context guard, so sample.estimated_total equals the guard's number exactly.
- A2+A6: capture-baseline bin (thin wrapper over bhippi_app::token_baseline, which is a lib
  module because bin targets are separate crate roots and the turn machinery is pub(crate)) runs
  5 representative tasks on the offline demo provider and writes docs/token-engine/baseline.md
  + baseline.json. Rerun with: cargo run -p bhippi-app --bin capture-baseline. Replays from an
  empty log so every run is deterministic.
- A3: measured = 0 tokens. Chat requests inject no tool schemas today; that zero is the A3 line,
  and must be re-measured whenever schemas ship.
- A4: 0 handoffs observed in the single-provider baseline; injecting the note costs 81 estimated
  tokens (template measured in token_baseline::handoff_note_tokens). Deviation: run_turn compared
  the prior provider *label* against the current *id* and picked up the turn's own queued row
  (which carries the provider label), so every turn — including a conversation's first — carried
  a bogus wrap directive. Fixed to compare label vs label and asserted by the baseline test
  (handoff_observed == 0 on single-provider runs).
- A5: workspace 272 / project_rules 199 estimated tokens on the fixture repo, per-turn fixed
  cost; engine map 0 for a no-scene workspace. These rows belong to this plan's Phase D/E work.
- Computer-Use request loops are not disaggregated yet: stream_requests is recorded as 1 for the
  initially assembled request (Phase G note). No SQL outside bhippi-db was touched.
```

---

# 53. FINAL INSTRUCTION TO THE IMPLEMENTING AI

Do not treat this document as a brainstorming document.

Treat it as an **implementation contract**.

Work in the defined phases.

After each task:

1. Implement.
2. Test.
3. Verify acceptance criteria.
4. Update the matching checkbox from `[ ]` to `[x]`.
5. Record important deviations under **Implementation Notes**.
6. Only then continue.

The priority order is:

```text
CORRECTNESS
    ↓
RELEVANT CONTEXT
    ↓
TOKEN EFFICIENCY
    ↓
LATENCY
    ↓
UI POLISH
```

Never sacrifice correctness merely to achieve a smaller token number.

The intended end state is:

> **Bhippi knows the project, scene, runtime, tasks, changes, and prior agent findings itself. The LLM is used primarily for judgment, planning, coding, debugging, and creative reasoning—not as a database, filesystem indexer, log parser, or message bus.**
