-- Stable ordering when multiple jobs share the same created_at (e.g. loop_n batch insert).
-- Pull uses ORDER BY created_at, queue_ordinal, id.

ALTER TABLE jobs
ADD COLUMN queue_ordinal INTEGER NOT NULL DEFAULT 0;
