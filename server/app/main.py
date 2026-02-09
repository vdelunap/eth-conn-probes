from fastapi import FastAPI, Request
from fastapi.responses import PlainTextResponse

from .geoip import lookup_ip
from .storage import append_report

app = FastAPI()


@app.get("/ping", response_class=PlainTextResponse)
def ping() -> str:
    return "ok"


def _client_ip(request: Request) -> str | None:
    """Best-effort client IP extraction.

    If you put this behind a reverse proxy later (nginx, cloud LB), you may want to
    trust X-Forwarded-For / Forwarded headers instead of request.client.host.
    """
    if request.client is None:
        return None
    return request.client.host


@app.post("/report")
async def report(request: Request):
    body = await request.json()

    ip = _client_ip(request)
    geo = lookup_ip(ip) if ip else None

    # The server enriches the payload. Don't trust client-supplied geo.
    body["_server"] = {
        "client_ip": ip,
        "user_agent": request.headers.get("user-agent"),
        "geo": geo,
    }

    append_report(body)
    return {"ok": True}
