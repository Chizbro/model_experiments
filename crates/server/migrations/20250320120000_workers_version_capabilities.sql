-- Worker registry: semver gate metadata (plan task 09).
ALTER TABLE workers
ADD COLUMN client_version TEXT;

ALTER TABLE workers
ADD COLUMN capabilities JSONB NOT NULL DEFAULT '[]'::jsonb;
