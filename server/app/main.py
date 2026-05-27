import json
import uuid
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse, PlainTextResponse
from slowapi import Limiter, _rate_limit_exceeded_handler
from slowapi.errors import RateLimitExceeded

from .db import close_pool, create_pool, get_pool
from .geoip import lookup_ip
from .storage import fetch_geo_reports, insert_report

# 512 KB is far more than any legitimate report will ever be
MAX_BODY_BYTES = 512 * 1024

# Hard cap on probe results to prevent memory exhaustion during parsing
MAX_PROBE_RESULTS = 500


# ---------------------------------------------------------------------------
# Rate limiter
# ---------------------------------------------------------------------------

def _rate_limit_key(request: Request) -> str:
    """Return the IP to rate-limit against.

    Trusts X-Forwarded-For only when the direct TCP connection comes from
    loopback (i.e. nginx on the same machine).  External clients cannot
    spoof their IP via that header this way.
    """
    direct = request.client.host if request.client else "unknown"
    if direct in ("127.0.0.1", "::1"):
        forwarded = request.headers.get("x-forwarded-for", "")
        if forwarded:
            return forwarded.split(",")[0].strip()
    return direct


limiter = Limiter(key_func=_rate_limit_key)


# ---------------------------------------------------------------------------
# App lifecycle
# ---------------------------------------------------------------------------

@asynccontextmanager
async def lifespan(_app: FastAPI):
    await create_pool()
    yield
    await close_pool()


app = FastAPI(lifespan=lifespan)
app.state.limiter = limiter
app.add_exception_handler(RateLimitExceeded, _rate_limit_exceeded_handler)


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

@app.get("/ping", response_class=PlainTextResponse)
def ping() -> str:
    return "ok"


@app.post("/report")
@limiter.limit("10/minute")
async def report(request: Request):
    # 1. Size guard — read raw bytes once; Starlette caches the body.
    body_bytes = await request.body()
    if len(body_bytes) > MAX_BODY_BYTES:
        return JSONResponse(status_code=413, content={"error": "Request body too large"})

    # 2. Parse JSON
    try:
        body = json.loads(body_bytes)
    except json.JSONDecodeError:
        return JSONResponse(status_code=400, content={"error": "Invalid JSON"})

    if not isinstance(body, dict):
        return JSONResponse(status_code=400, content={"error": "Expected a JSON object"})

    # 3. Require a valid UUID as run_id (rejects garbage / fuzzing payloads early)
    if not _is_uuid(body.get("run_id")):
        return JSONResponse(status_code=400, content={"error": "Missing or invalid run_id"})

    # 4. Cap results array length
    results = body.get("results")
    if isinstance(results, list) and len(results) > MAX_PROBE_RESULTS:
        return JSONResponse(status_code=400, content={"error": "Too many probe results"})

    # 5. Server-side enrichment — always overwrite whatever the client sent
    ip = request.client.host if request.client else None
    body["_server"] = {
        "client_ip": ip,
        "user_agent": request.headers.get("user-agent"),
        "geo": lookup_ip(ip) if ip else None,
    }

    # 6. Persist
    inserted = await insert_report(body, get_pool())
    return {"ok": True, "new": inserted}


# ---------------------------------------------------------------------------
# Map data endpoint (ready for MapLibre — not yet wired to the app)
# ---------------------------------------------------------------------------

@app.get("/api/geo-reports")
@limiter.limit("30/minute")
async def geo_reports(request: Request):
    rows = await fetch_geo_reports(get_pool())
    features = [
        {
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [r["lon"], r["lat"]]},
            "properties": {
                "country_iso":  r["country_iso"],
                "country_name": r["country_name"],
                "city_name":    r["city_name"],
                "received_at":  r["received_at"].isoformat(),
                "ok_count":     r["ok_count"],
                "fail_count":   r["fail_count"],
            },
        }
        for r in rows
    ]
    return {"type": "FeatureCollection", "features": features}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _is_uuid(value: object) -> bool:
    try:
        uuid.UUID(str(value))
        return True
    except (ValueError, AttributeError, TypeError):
        return False
