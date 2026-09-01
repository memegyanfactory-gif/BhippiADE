# Bhippi — Runtime Pipelines
**Doc:** `05-PIPELINES.md` · **Derives from:** spec §3.2, §9–§17 · **Status:** authoritative

The end-to-end flows, step by step, with the owning crate, the input, the committed output,
and the gate that can stop it. When you implement a stage, this is the row you are
satisfying. Every step emits a `tracing` span carrying the session id.

---

## P1 · Research session (the main flow)

```
seed --> PLAN --> EXPAND(loop) --> SYNTHESISE --> FACT GATE --> WRITE --> IMAGE
                     ^   |                                                  |
                     |   v                                                  v
              frontier <- MIND MAP                     MEMORY <- GIST <-- SEO --> PUBLISH
```

| # | Step | Crate | In | Out (committed) | Gate / stop condition |
|---|---|---|---|---|---|
| 1 | Seed intake | core | topic, tier, origin | `sessions` row, `status=planning` | budget guard green; no session running |
| 2 | Memory retrieval | memory | seed | `PriorKnowledge` (in-memory) | block ≤ 6 % of planner context |
| 3 | Plan | research | seed + prior | charter JSON on session | **domain gate**: out of scope ⇒ `rejected` |
| 4 | Frontier init | research | charter | seed node, hop 0 | — |
| 5 | Pick node | research | frontier | node `status=expanding` | budget: expansions, depth, tokens, wall |
| 6 | Discover | harvest | node + charter queries | candidate URLs | feeds → search → links → **primary jump** |
| 7 | Fetch | harvest | URLs | `sources` rows + blobs | robots, rate limit, paywall, 4 MB cap |
| 8 | Extract | harvest | raw | extracted text + candidates | thin/headless rules |
| 9 | Dedupe | harvest | extracted | canonical / hash / simhash verdict | dupes become corroborations |
| 10 | Extract dots | research | node + text | `dots` rows | no provenance ⇒ dropped; quote caps enforced |
| 11 | Derive children | research | node + dots | candidate nodes | — |
| 12 | Score + drift guard | research | children | `nodes` with priority | cosine ≥ 0.45 to seed or justified counterpoint |
| 13 | Commit expansion | core+db | all of the above | one transaction, `stage_cursor` bumped | **this is the resume point** |
| 14 | Loop | core | — | — | back to 5 until budget or frontier exhausted |
| 15 | Counter-evidence pass | research | strongest claims | `counterpoint` nodes | run `tier.counter_passes` times |
| 16 | Timeline (X12+) | research | dated dots | `timeline` nodes | — |
| 17 | Floors check | core | source/primary counts | `flags.thin_evidence` | below floor ⇒ **forced review** |
| 18 | Synthesise | research | mind map + memory | blueprint JSON | `unknowns` non-empty at X12/X24 |
| 19 | Fact gate | research | blueprint + map | fact report, `fact_score` | < 70 ⇒ mandatory human review |
| 20 | Headlines + hooks | writer | blueprint | 12 headlines, 5 hooks, scores | hook claim maps to a dot ≥ 0.8 confidence |
| 21 | Section drafts | writer | section plan + its dots + 200-tok ctx | sections | — |
| 22 | Weld | writer | sections | transitions | open loops closed/opened |
| 23 | Editor pass | writer | draft | editor report | **different provider than Writer** |
| 24 | Style lint | writer | draft | lint report | any hard failure ⇒ block |
| 25 | Images | vision | image intents per section | `images` rows + variants | unresolved licence ⇒ rejected |
| 26 | Keywords + metadata | seo | blueprint + corpus | keyword set, metadata, slug | density 0.6–1.6 % |
| 27 | Internal links | seo | post + corpus | `link_edits` rows | 2–4 out, 1–2 in, revertible |
| 28 | Assemble `post.json` | seo | everything | `posts` row (`draft`) | Appendix A schema valid |
| 29 | Gate to review or publish | core | flags + config | `review` or `publishing` | review gate · thin evidence · fact < 70 |
| 30 | Build | publish | posts + theme | temp bundle | — |
| 31 | Verify | publish | bundle | verify report | **any §14.6 failure blocks** |
| 32 | Deploy | publish | bundle | atomic swap, `deploy_ref`, `deploys` row | preflight (creds, connectivity, quota) |
| 33 | Gist | memory | session summary | `memory_gists` row | ≤ 1200 tokens, dead ends mandatory |
| 34 | Learn | memory | outcomes | `domain_stats`, `query_stats`, `interest_weights`, `style_prefs` | — |
| 35 | Metrics | core | timings | `session_metrics`, `status=done` | — |

**Resume rule:** restart re-enters at `stage_cursor`. Steps 6–13 never re-fetch a URL already
present in `sources` for that session.

---

## P2 · Harvest (per URL)

```
url -> canonicalise -> robots check -> host governor -> HTTP GET (8s/20s, 4MB)
    -> content-type route (html | pdf | json)
    -> charset normalise -> boilerplate strip -> metadata (JSON-LD/OG/byline)
    -> main text (markdown) -> tables/code preserved -> image candidates -> link inventory
    -> thin? (<400 chars && >=8 scripts) -> headless retry (once, 15s, <=15% of session)
    -> paywall detect -> if paywalled: keep abstract only, STOP
    -> blob write -> content_hash + simhash -> dedupe verdict -> sources row
```

Failure routing: 4xx ⇒ no retry, record. 5xx/timeout ⇒ 3 jittered retries. 429/503 ⇒ honour
`Retry-After` exactly, host cooldown. Robots disallow ⇒ **zero requests**, recorded as skipped.

---

## P3 · Ticker

```
stagger timer -> poll feed (ETag/IMS) -> parse -> canonicalise -> seen? drop
   -> domain classifier (tech/AI) -> below threshold? discard
   -> category assign -> cluster (title simhash + entity overlap + 6h window)
   -> burst count (distinct domains) -> velocity (outlets/hour over 3h)
   -> priority score -> ticker_events row -> emit event
   -> automation evaluator: all 9 trigger conditions? -> debounce 5 min -> enqueue session
```

Circuit breaker: 5 consecutive feed errors ⇒ open, surfaced in Settings › Ticker, retried
with backoff. Network loss ⇒ strip goes amber, recovers with **no duplicates**.

---

## P4 · Image

```
section image_intent -> candidates (press kit -> open licence -> open-access figure -> generate)
   -> licence resolve (named permission or REJECT)
   -> vision understanding (JSON: subject_bbox, safe_crop_region, kind, quality, relevance, alt)
   -> reject rules (relevance<0.55, watermark, sharpness<0.35, upscaled, unreadable, private person)
   -> EXIF strip -> phash dedupe (<=6) -> saliency map -> reconcile with subject_bbox
   -> focal point -> crop set (never cutting safe region; diagrams letterboxed, not cropped)
   -> resize Lanczos3 -> encode AVIF q60 + WebP q78 + JPEG q82 -> srcset [400,800,1200,1600]
   -> variants JSON -> images row (approved)
```

If every candidate fails, the engine **renders its own diagram from dot data** and the post
still builds.

---

## P5 · Publish

```
post.json set -> renderer (static minijinja | React SSG) -> temp dir bundle
   -> VERIFY: internal links · image variants · licences · style lint · fact_score
              · slug uniqueness · meta description · AI disclosure · Lighthouse SEO/Perf
   -> preflight target (creds, connectivity, quota)
   -> atomic swap (temp -> live) -> deploy_ref -> deploys row -> sitemap/RSS/feed refresh
   -> internal-link back-insertion into older posts (recorded in link_edits)
```

Rollback restores the previous `deploy_ref` in one command. Power loss leaves the old site or
the new one — never a half-written one.

---

## P6 · Automation tick

```
cron tick -> guards (mode, quiet hours, daily cap, budget, crash-loop) 
   -> topic picker:
        1. highest-priority uncovered ticker cluster (last 24 h)
        2. highest-weight coverage-heat gap
        3. refresh: post > 30 days old whose entities moved  -> UPDATE IN PLACE
        4. user queue
      (never a topic covered in the last 14 days unless refresh)
   -> duplicate guard (slug + embedding >= 0.93 ⇒ refresh instead of new)
   -> enqueue job -> single-session permit -> P1
```

---

## P7 · Memory

```
session done -> gist writer (angle, established, disputed, unknown, entities,
                             sources that paid off, DEAD ENDS) -> embed -> index
            -> entity upsert + entity_links
            -> domain_stats / query_stats / interest_weights / style_prefs updates
daily      -> decay tick: decay_score *= 0.5^(days_idle / half_life)
            -> below 0.15 && unpinned && 180 days ⇒ archive (not delete)
new run    -> hybrid retrieve (0.6 vector + 0.4 BM25, top 8) + entity subgraph (2 hops)
            -> PRIOR KNOWLEDGE block with explicit staleness markers
            -> planner instructed: prior to verify, never ground truth to repeat
```

---

## P8 · Skills

```
telemetry -> procedure repeated >= 5 times with stable shape
   -> SkillAuthor drafts manifest + body + >= 10 eval cases from real sessions
   -> evaluate in sandbox: correctness x schema-validity x latency-improvement
   -> score >= min_score ⇒ autonomy = trial
   -> shadow-compare against baseline for 20 real runs
   -> win-rate >= 60 % AND autonomy gate (script/net/fs_write ⇒ USER APPROVAL) ⇒ enabled
   -> monitor: 3 consecutive failures or -15 % score ⇒ quarantine + notify
```

The baseline path always remains available, so a bad skill degrades speed, never correctness.

---

## P9 · Provider call (every single one)

```
TaskClass -> budget guard (reject BEFORE issuing if cap exceeded)
   -> router: health x caps x routing policy x Editor!=Writer pin
   -> prompt file (versioned, hash pinned into prompt_versions + posts.prompt_hashes)
   -> untrusted content wrapped in a data block
   -> call with timeout + cancellation token
   -> structured output? validate against JSON Schema
        -> invalid: ONE repair round-trip -> still invalid: reject
   -> on error/timeout: retry same provider once (backoff) -> next candidate -> max 3
   -> record tokens, latency, provider, task class into session_metrics
   -> replay dump: prompt + input + output to ~/.bhippi/replay/<session>/
```

**Replay is built in sprint 1, not at the end.** It is how quality regressions get debugged.
