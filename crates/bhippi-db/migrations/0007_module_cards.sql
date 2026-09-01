PRAGMA foreign_keys = ON;

-- Module knowledge cards (Phase B8, plan SEC. 6).
--
-- A compact, deterministic summary of one module's public surface, precomputed from
-- the structural symbol index.  A module here maps to one indexed source file, keyed
-- by its path without extension.  Facts below are hard data derived from the index
-- (never AI-generated); an optional `description` is stored separately and carries
-- provenance via `description_origin` so generated claims can never be mistaken for
-- hard facts.  `card_revision` records the max symbol `source_revision` the card was
-- built at, so a rescan recomputes only cards whose file actually changed.

CREATE TABLE brain_module_cards (
  project_id        TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  module_name       TEXT NOT NULL,                 -- rel_path sans extension, e.g. "src/lib"
  entry_points      TEXT NOT NULL DEFAULT '[]',    -- JSON array of qualified names (top-level fns)
  public_symbols    TEXT NOT NULL DEFAULT '[]',    -- JSON array of qualified names (top-level items)
  symbol_count      INTEGER NOT NULL DEFAULT 0,
  description       TEXT,                          -- optional AI description; NULL when absent
  description_origin TEXT,                         -- provenance for the description claim
  card_revision     INTEGER NOT NULL DEFAULT 0,    -- max symbol source_revision card was built at
  updated_at        TEXT NOT NULL,
  PRIMARY KEY (project_id, module_name)
);
