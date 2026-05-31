import configparser
import os
from typing import Optional

import asyncpg

_pool: Optional[asyncpg.Pool] = None


def _connection_params() -> dict:
    """Connection kwargs from the 'main' PGSERVICEFILE entry, or DATABASE_URL locally."""
    pgservicefile = os.environ.get("PGSERVICEFILE")
    if pgservicefile and os.path.exists(pgservicefile):
        cfg = configparser.ConfigParser()
        cfg.read(pgservicefile)
        if "main" in cfg:
            svc = cfg["main"]
            params: dict = {
                "host":     svc.get("host", "postgres"),
                "port":     int(svc.get("port", 5432)),
                "database": svc.get("dbname", "dbalfa"),
                "user":     svc.get("user", "ethprobes"),
            }
            if "password" in svc:
                params["password"] = svc["password"]
            return params
    return {"dsn": os.environ["DATABASE_URL"]}


async def create_pool() -> None:
    global _pool
    _pool = await asyncpg.create_pool(
        **_connection_params(),
        min_size=1,
        max_size=5,
        command_timeout=10,
        server_settings={"search_path": "ethconnprobes"},
    )


async def close_pool() -> None:
    global _pool
    if _pool is not None:
        await _pool.close()
        _pool = None


def get_pool() -> asyncpg.Pool:
    if _pool is None:
        raise RuntimeError("DB pool not initialised; lifespan not running?")
    return _pool
