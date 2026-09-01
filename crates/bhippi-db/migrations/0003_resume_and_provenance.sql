ALTER TABLE sessions ADD COLUMN charter TEXT;
ALTER TABLE sessions ADD COLUMN blueprint TEXT;
ALTER TABLE sessions ADD COLUMN writer_provider TEXT;
ALTER TABLE sessions ADD COLUMN flags TEXT;
ALTER TABLE posts ADD COLUMN disclosure TEXT NOT NULL DEFAULT '{"generated":true,"reviewed_by_human":false}';
ALTER TABLE posts ADD COLUMN correction TEXT;
ALTER TABLE sources ADD COLUMN learned_trust_at_fetch REAL;
