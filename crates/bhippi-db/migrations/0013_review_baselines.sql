PRAGMA foreign_keys = ON;

-- The review ledger. "Review Changes" and the +n −n counters used to read `git diff`, and a
-- game Bhippi builds is not a git repository, so a whole game could be written and the
-- review would say the workspace was clean. This table is Bhippi's own record of what it
-- changed: the first time any write path touches a file, the text that was there before is
-- kept here. A review is then the ledger's baseline against the disk, and it works in every
-- project, git or not.
--
--   file_path    absolute, forward-slashed — the key, so the same file reached through two
--                workspace spellings is one row
--   project_path the workspace the write belonged to, forward-slashed
--   rel_path     the workspace-relative name the transcript prints
--   existed      0 when the file did not exist before the first write (a creation)
--   before_text  the pre-write text; NULL for a creation or for a file that is not text
--
-- First touch wins: a later write never replaces a baseline, because the baseline is the
-- state before Bhippi started, not before its latest step.

CREATE TABLE review_baselines (
  file_path    TEXT NOT NULL PRIMARY KEY,
  project_path TEXT NOT NULL,
  rel_path     TEXT NOT NULL,
  existed      INTEGER NOT NULL CHECK (existed IN (0, 1)),
  before_text  TEXT,
  recorded_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_review_baselines_project
  ON review_baselines (project_path, rel_path);
