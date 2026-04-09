-- Ethereum Connection Probes — PostgreSQL schema
-- Executed automatically by the postgres container on first start
-- (mounted at /docker-entrypoint-initdb.d/01_schema.sql)

-- ---------------------------------------------------------------------------
-- reports
-- One row per submitted run. run_id is the client-generated UUID; the UNIQUE
-- constraint silently rejects duplicates so retries from the app are safe.
-- ---------------------------------------------------------------------------
CREATE TABLE reports (
    id              BIGSERIAL PRIMARY KEY,
    received_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Report struct (client-supplied)
    run_id          UUID        NOT NULL,
    client_ts       TIMESTAMPTZ,
    started_at_ms   BIGINT,
    finished_at_ms  BIGINT,

    -- ClientInfo
    os              TEXT,
    arch            TEXT,
    client_id       UUID,
    app_channel     TEXT,

    -- RunConfig (parameters used for this run)
    cfg_attempts        SMALLINT,
    cfg_min_successes   SMALLINT,
    cfg_timeout_ms      INTEGER,
    cfg_parallelism     SMALLINT,

    -- Server-enriched (never trust client-supplied values for these)
    client_ip       TEXT,
    user_agent      TEXT,

    -- GeoIP (MaxMind GeoLite2, resolved server-side)
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
CREATE TABLE probe_runs (
    id          BIGSERIAL PRIMARY KEY,
    report_id   BIGINT NOT NULL REFERENCES reports(id) ON DELETE CASCADE,

    kind        TEXT    NOT NULL,   -- ProbeKind variant, e.g. "HttpsJsonRpc"
    target      TEXT    NOT NULL,   -- hostname, URL, ENR, etc.

    -- ProbeSummary (aggregated across all attempts for this probe/target)
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
CREATE TABLE probe_attempts (
    id              BIGSERIAL PRIMARY KEY,
    probe_run_id    BIGINT  NOT NULL REFERENCES probe_runs(id) ON DELETE CASCADE,

    attempt_index   SMALLINT    NOT NULL,
    ok              BOOLEAN     NOT NULL,
    rtt_ms          BIGINT,
    error           TEXT,
    meta            JSONB   -- probe-specific metadata; schema varies by kind
);

-- ---------------------------------------------------------------------------
-- Indexes
-- ---------------------------------------------------------------------------
CREATE INDEX reports_received_at_idx    ON reports       (received_at DESC);
CREATE INDEX reports_client_id_idx      ON reports       (client_id);
CREATE INDEX probe_runs_report_id_idx   ON probe_runs    (report_id);
CREATE INDEX probe_runs_kind_idx        ON probe_runs    (kind);
CREATE INDEX probe_attempts_run_id_idx  ON probe_attempts (probe_run_id);
