-- eth-conn-probes: PostgreSQL base schema
-- Executed by init/dbinit.sh on first container start.
-- Passwords are NOT set here; they are altered in secrets/postgres/auth.sql.

-- ---------------------------------------------------------------------------
-- Users (no passwords; set via auth.sql)
-- ---------------------------------------------------------------------------
CREATE USER ethprobes;
CREATE USER dbuser;

-- ---------------------------------------------------------------------------
-- Schema, created as admin (POSTGRES_USER / superuser)
-- ---------------------------------------------------------------------------
SET SESSION AUTHORIZATION admin;
CREATE SCHEMA IF NOT EXISTS ethconnprobes;

-- Default privileges: tables created by admin in ethconnprobes automatically
-- grant the right permissions to ethprobes and dbuser.
ALTER DEFAULT PRIVILEGES IN SCHEMA ethconnprobes GRANT ALL      ON TABLES    TO ethprobes;
ALTER DEFAULT PRIVILEGES IN SCHEMA ethconnprobes GRANT ALL      ON SEQUENCES TO ethprobes;
ALTER DEFAULT PRIVILEGES IN SCHEMA ethconnprobes GRANT SELECT   ON TABLES    TO dbuser;

-- Schema and database access
GRANT CONNECT ON DATABASE dbalfa         TO ethprobes, dbuser;
GRANT USAGE   ON SCHEMA   ethconnprobes  TO ethprobes, dbuser;

-- Role-level search path so connections get ethconnprobes by default
ALTER ROLE ethprobes SET search_path = ethconnprobes;
ALTER ROLE dbuser    SET search_path = ethconnprobes;

-- ---------------------------------------------------------------------------
-- reports
-- One row per submitted run. run_id is the client-generated UUID; the UNIQUE
-- constraint silently rejects duplicates so retries from the app are safe.
-- ---------------------------------------------------------------------------
CREATE TABLE ethconnprobes.reports (
    id              BIGSERIAL PRIMARY KEY,
    received_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    run_id          UUID        NOT NULL,
    client_ts       TIMESTAMPTZ,
    started_at_ms   BIGINT,
    finished_at_ms  BIGINT,

    os              TEXT,
    arch            TEXT,
    client_id       UUID,
    app_channel     TEXT,

    cfg_attempts        SMALLINT,
    cfg_min_successes   SMALLINT,
    cfg_timeout_ms      INTEGER,
    cfg_parallelism     SMALLINT,

    client_ip       TEXT,
    user_agent      TEXT,
    network_label   TEXT,

    geo_country_iso         CHAR(2),
    geo_country_name        TEXT,
    geo_region_name         TEXT,
    geo_city_name           TEXT,
    geo_postal_code         TEXT,
    geo_timezone            TEXT,
    geo_latitude            DOUBLE PRECISION,
    geo_longitude           DOUBLE PRECISION,
    geo_accuracy_radius_km  INTEGER,
    geo_asn                 INTEGER,
    geo_as_org              TEXT,

    CONSTRAINT reports_run_id_unique UNIQUE (run_id)
);

-- ---------------------------------------------------------------------------
-- probe_runs
-- One row per (probe kind × target) within a report.
-- ---------------------------------------------------------------------------
CREATE TABLE ethconnprobes.probe_runs (
    id          BIGSERIAL PRIMARY KEY,
    report_id   BIGINT NOT NULL REFERENCES ethconnprobes.reports(id) ON DELETE CASCADE,

    kind        TEXT    NOT NULL,
    target      TEXT    NOT NULL,

    success_count   SMALLINT    NOT NULL DEFAULT 0,
    failure_count   SMALLINT    NOT NULL DEFAULT 0,
    min_rtt_ms      BIGINT,
    avg_rtt_ms      BIGINT,
    max_rtt_ms      BIGINT,
    ok              BOOLEAN     NOT NULL DEFAULT FALSE
);

-- ---------------------------------------------------------------------------
-- probe_attempts
-- One row per individual attempt within a probe run.
-- ---------------------------------------------------------------------------
CREATE TABLE ethconnprobes.probe_attempts (
    id              BIGSERIAL PRIMARY KEY,
    probe_run_id    BIGINT  NOT NULL REFERENCES ethconnprobes.probe_runs(id) ON DELETE CASCADE,

    attempt_index   SMALLINT    NOT NULL,
    ok              BOOLEAN     NOT NULL,
    rtt_ms          BIGINT,
    error           TEXT,
    meta            JSONB
);

-- ---------------------------------------------------------------------------
-- Indexes
-- ---------------------------------------------------------------------------
CREATE INDEX reports_received_at_idx    ON ethconnprobes.reports       (received_at DESC);
CREATE INDEX reports_client_id_idx      ON ethconnprobes.reports       (client_id);
CREATE INDEX probe_runs_report_id_idx   ON ethconnprobes.probe_runs    (report_id);
CREATE INDEX probe_runs_kind_idx        ON ethconnprobes.probe_runs    (kind);
CREATE INDEX probe_attempts_run_id_idx  ON ethconnprobes.probe_attempts (probe_run_id);

-- ---------------------------------------------------------------------------
-- Explicit grants for the tables created above
-- (default privileges cover future tables; these cover the tables just created)
-- ---------------------------------------------------------------------------
GRANT ALL    ON ALL TABLES    IN SCHEMA ethconnprobes TO ethprobes;
GRANT ALL    ON ALL SEQUENCES IN SCHEMA ethconnprobes TO ethprobes;
GRANT SELECT ON ALL TABLES    IN SCHEMA ethconnprobes TO dbuser;