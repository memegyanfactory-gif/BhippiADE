PRAGMA foreign_keys = ON;

-- Semantic index (Phase B5). Adds deterministic token-hash embeddings to symbols
-- plus a per-project record of which embedding model/version the index was built
-- with, so a model bump triggers a full re-embed instead of mixing feature spaces.

ALTER TABLE brain_symbols ADD COLUMN embedding_blob  BLOB;
ALTER TABLE brain_symbols ADD COLUMN embedding_dim   INTEGER;
ALTER TABLE brain_symbols ADD COLUMN embedding_model TEXT;

CREATE TABLE brain_embedding_state (
  project_id  TEXT PRIMARY KEY REFERENCES brain_projects(id) ON DELETE CASCADE,
  model       TEXT NOT NULL,                 -- embedding model id the index was built with
  updated_at  TEXT NOT NULL
);
