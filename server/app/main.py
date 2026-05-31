import json
import uuid
from contextlib import asynccontextmanager
from typing import Optional

from fastapi import FastAPI, Query, Request
from fastapi.responses import JSONResponse, PlainTextResponse
from slowapi import Limiter, _rate_limit_exceeded_handler
from slowapi.errors import RateLimitExceeded

from .db import close_pool, create_pool, get_pool
from .geoip import lookup_ip
from .storage import fetch_geo_reports, insert_report

MAX_BODY_BYTES = 512 * 1024
MAX_PROBE_RESULTS = 500


def _rate_limit_key(request: Request) -> str:
    # Behind a local reverse proxy every request looks like loopback, so fall back
    # to X-Forwarded-For in that case only, since it's spoofable from anywhere else.
    direct = request.client.host if request.client else "unknown"
    if direct in ("127.0.0.1", "::1"):
        forwarded = request.headers.get("x-forwarded-for", "")
        if forwarded:
            return forwarded.split(",")[0].strip()
    return direct


limiter = Limiter(key_func=_rate_limit_key)


@asynccontextmanager
async def lifespan(_app: FastAPI):
    await create_pool()
    yield
    await close_pool()


app = FastAPI(lifespan=lifespan)
app.state.limiter = limiter
app.add_exception_handler(RateLimitExceeded, _rate_limit_exceeded_handler)


@app.get("/ping", response_class=PlainTextResponse)
def ping() -> str:
    return "ok"


@app.post("/report")
@limiter.limit("10/minute")
async def report(request: Request):
    body_bytes = await request.body()
    if len(body_bytes) > MAX_BODY_BYTES:
        return JSONResponse(status_code=413, content={"error": "Request body too large"})

    try:
        body = json.loads(body_bytes)
    except json.JSONDecodeError:
        return JSONResponse(status_code=400, content={"error": "Invalid JSON"})

    if not isinstance(body, dict):
        return JSONResponse(status_code=400, content={"error": "Expected a JSON object"})

    if not _is_uuid(body.get("run_id")):
        return JSONResponse(status_code=400, content={"error": "Missing or invalid run_id"})

    results = body.get("results")
    if isinstance(results, list) and len(results) > MAX_PROBE_RESULTS:
        return JSONResponse(status_code=400, content={"error": "Too many probe results"})

    ip = request.client.host if request.client else None
    body["_server"] = {
        "client_ip": ip,
        "user_agent": request.headers.get("user-agent"),
        "geo": lookup_ip(ip) if ip else None,
    }

    inserted = await insert_report(body, get_pool())
    return {"ok": True, "new": inserted}


@app.get("/api/geo-reports")
@limiter.limit("30/minute")
async def geo_reports(
    request: Request,
    kinds: Optional[str] = Query(default=None, description="Comma-separated probe kinds to filter"),
):
    kinds_list = [k.strip() for k in kinds.split(",") if k.strip()] if kinds else None
    rows = await fetch_geo_reports(get_pool(), kinds=kinds_list)
    features = []
    for r in rows:
        ok = int(r["ok_count"] or 0)
        fail = int(r["fail_count"] or 0)
        total = ok + fail
        features.append({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [r["lon"], r["lat"]]},
            "properties": {
                "country_iso":    r["country_iso"],
                "country_name":   r["country_name"],
                "city_name":      r["city_name"],
                "network_label":  r["network_label"],
                "received_at":    r["received_at"].isoformat(),
                "ok_count":       ok,
                "fail_count":     fail,
                "total":          total,
            },
        })
    return {"type": "FeatureCollection", "features": features}


def _is_uuid(value: object) -> bool:
    try:
        uuid.UUID(str(value))
        return True
    except (ValueError, AttributeError, TypeError):
        return False
