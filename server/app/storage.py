import json
import uuid
from datetime import datetime
from typing import Any, Optional

import asyncpg


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _to_uuid(value: Any) -> Optional[uuid.UUID]:
    if value is None:
        return None
    try:
        return uuid.UUID(str(value))
    except (ValueError, AttributeError):
        return None


def _parse_ts(ts: Optional[str]) -> Optional[datetime]:
    if not ts:
        return None
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00"))
    except (ValueError, AttributeError):
        return None


def _int(v: Any) -> Optional[int]:
    try:
        return int(v) if v is not None else None
    except (ValueError, TypeError):
        return None


def _float(v: Any) -> Optional[float]:
    try:
        return float(v) if v is not None else None
    except (ValueError, TypeError):
        return None


def _str(v: Any) -> Optional[str]:
    return str(v) if v is not None else None


# ---------------------------------------------------------------------------
# Main insert
# ---------------------------------------------------------------------------

async def insert_report(body: dict, pool: asyncpg.Pool) -> bool:
    """Persist a full report inside a single transaction.

    Returns True when inserted, False when the run_id already existed
    (the app's retry logic may re-submit; that's handled gracefully).
    """
    server   = body.get("_server") or {}
    geo      = server.get("geo") or {}
    client   = body.get("client") or {}
    run_cfg  = body.get("run") or {}
    results  = body.get("results") or []

    async with pool.acquire() as conn:
        async with conn.transaction():

            # -- reports -------------------------------------------------------
            report_id: Optional[int] = await conn.fetchval(
                """
                INSERT INTO reports (
                    run_id, client_ts, started_at_ms, finished_at_ms,
                    os, arch, client_id, app_channel,
                    cfg_attempts, cfg_min_successes, cfg_timeout_ms, cfg_parallelism,
                    client_ip, user_agent,
                    geo_country_iso, geo_country_name, geo_region_name, geo_city_name,
                    geo_postal_code, geo_timezone,
                    geo_latitude, geo_longitude, geo_accuracy_radius_km,
                    geo_asn, geo_as_org
                ) VALUES (
                    $1,  $2,  $3,  $4,
                    $5,  $6,  $7,  $8,
                    $9,  $10, $11, $12,
                    $13, $14,
                    $15, $16, $17, $18,
                    $19, $20,
                    $21, $22, $23,
                    $24, $25
                )
                ON CONFLICT (run_id) DO NOTHING
                RETURNING id
                """,
                _to_uuid(body.get("run_id")),
                _parse_ts(body.get("timestamp")),
                _int(body.get("started_at_ms")),
                _int(body.get("finished_at_ms")),
                _str(client.get("os")),
                _str(client.get("arch")),
                _to_uuid(client.get("client_id")),
                _str(client.get("app_channel")),
                _int(run_cfg.get("attempts")),
                _int(run_cfg.get("min_successes")),
                _int(run_cfg.get("timeout_ms")),
                _int(run_cfg.get("parallelism")),
                _str(server.get("client_ip")),
                _str(server.get("user_agent")),
                _str(geo.get("country_iso")),
                _str(geo.get("country_name")),
                _str(geo.get("region_name")),
                _str(geo.get("city_name")),
                _str(geo.get("postal_code")),
                _str(geo.get("timezone")),
                _float(geo.get("latitude")),
                _float(geo.get("longitude")),
                _int(geo.get("accuracy_radius_km")),
                _int(geo.get("asn")),
                _str(geo.get("as_org")),
            )

            if report_id is None:
                return False  # duplicate run_id — nothing to do

            # -- probe_runs + probe_attempts ------------------------------------
            for probe_run in results:
                summary = probe_run.get("summary") or {}

                probe_run_id: int = await conn.fetchval(
                    """
                    INSERT INTO probe_runs (
                        report_id, kind, target,
                        success_count, failure_count,
                        min_rtt_ms, avg_rtt_ms, max_rtt_ms, ok
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    RETURNING id
                    """,
                    report_id,
                    _str(probe_run.get("kind")),
                    _str(probe_run.get("target")),
                    _int(summary.get("success_count")) or 0,
                    _int(summary.get("failure_count")) or 0,
                    _int(summary.get("min_rtt_ms")),
                    _int(summary.get("avg_rtt_ms")),
                    _int(summary.get("max_rtt_ms")),
                    bool(summary.get("ok", False)),
                )

                for idx, attempt in enumerate(probe_run.get("attempts") or []):
                    meta = attempt.get("meta")
                    await conn.execute(
                        """
                        INSERT INTO probe_attempts (
                            probe_run_id, attempt_index, ok, rtt_ms, error, meta
                        ) VALUES ($1, $2, $3, $4, $5, $6::jsonb)
                        """,
                        probe_run_id,
                        idx,
                        bool(attempt.get("ok", False)),
                        _int(attempt.get("rtt_ms")),
                        _str(attempt.get("error")),
                        json.dumps(meta) if meta is not None else None,
                    )

    return True
