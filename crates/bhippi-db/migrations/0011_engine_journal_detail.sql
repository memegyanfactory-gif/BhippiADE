PRAGMA foreign_keys = ON;

-- ENG-103. `0004_engine.sql` shipped the journal tables in 2026-08-29 and nothing has
-- ever written a row to them, so INV-071 ("every applied transaction is journaled with
-- actor + label") has been unenforced since. Before wiring the writer we need three
-- facts 0004 did not record:
--
--   scene_rel_path  which scene the transaction hit — a game has Main, a HUD and N
--                   levels, so a project-wide journal without it cannot answer
--                   "what did the agent change?" per scene, nor drive a per-scene undo.
--   inverse_json    the captured inverse, so undo survives a restart instead of dying
--                   with the in-memory UndoStack.
--   touched_json    the entity ids the transaction moved, so the UI can patch just those
--                   rows (INV-076 coalescing) instead of reloading the scene.
--
-- Defaults are constants because SQLite requires that of ALTER TABLE ADD COLUMN; existing
-- rows are impossible (the table has always been empty), so no backfill is needed.

ALTER TABLE engine_journal ADD COLUMN scene_rel_path TEXT NOT NULL DEFAULT '';
ALTER TABLE engine_journal ADD COLUMN inverse_json   TEXT NOT NULL DEFAULT '[]';
ALTER TABLE engine_journal ADD COLUMN touched_json   TEXT NOT NULL DEFAULT '[]';
ALTER TABLE engine_journal ADD COLUMN op_count       INTEGER NOT NULL DEFAULT 0;

-- Paging backwards through one scene's history is the common read.
CREATE INDEX IF NOT EXISTS idx_engine_journal_scene
  ON engine_journal (project_path, scene_rel_path, revision DESC);
