-- GAD-105 / ADR-0042: Drop retired research/publishing tables.
-- Preserves chat_turns, prompt_versions, incidents, skill_runs, jobs, dead_letters, skills, providers, and all Godot engine tables.

DROP TABLE IF EXISTS session_metrics;
DROP TABLE IF EXISTS style_prefs;
DROP TABLE IF EXISTS interest_weights;
DROP TABLE IF EXISTS query_stats;
DROP TABLE IF EXISTS domain_stats;
DROP TABLE IF EXISTS deploys;
DROP TABLE IF EXISTS redirects;
DROP TABLE IF EXISTS link_edits;
DROP TABLE IF EXISTS posts;
DROP TABLE IF EXISTS ticker_events;
DROP TABLE IF EXISTS entity_links;
DROP TABLE IF EXISTS entities;
DROP TABLE IF EXISTS memory_gists;
DROP TABLE IF EXISTS images;
DROP TABLE IF EXISTS source_registry;
DROP TABLE IF EXISTS dots;
DROP TABLE IF EXISTS edges;
DROP TABLE IF EXISTS nodes;
DROP TABLE IF EXISTS sources;
DROP TABLE IF EXISTS sessions;
