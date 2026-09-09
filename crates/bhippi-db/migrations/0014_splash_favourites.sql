PRAGMA foreign_keys = ON;

-- GAD-161. Favourited splash screens.
--
-- A splash belongs to one game — it is built into that project and boots it — but a splash
-- the author *likes* is a house style they will want on the next game too. So favourites are
-- kept here, per user and across projects, rather than inside any one game folder: a game
-- that is deleted must not take the studio's saved looks with it.
--
-- The spec is stored as its JSON rather than as columns. It is written and read only by
-- `bhippi-engine::godot::splash`, which owns its shape and versions it; spreading those
-- fields across columns would mean a migration every time a splash gains a setting, and
-- nothing here ever queries by one.
--
--   id         ULID, so favourites sort by when they were saved without a second column
--   name       what the author called it, shown in the panel's list
--   spec_json  the SplashSpec exactly as the engine serialises it
--   origin     the project it was saved from, forward-slashed, for "where did this come
--              from" — empty when the project is gone or was never named

CREATE TABLE splash_favourites (
  id         TEXT NOT NULL PRIMARY KEY,
  name       TEXT NOT NULL,
  spec_json  TEXT NOT NULL,
  origin     TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

-- The only read the panel makes: newest first.
CREATE INDEX IF NOT EXISTS idx_splash_favourites_recent
  ON splash_favourites (created_at DESC);
