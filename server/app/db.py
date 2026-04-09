import os
from typing import Optional

import asyncpg

_pool: Optional[asyncpg.Pool] = None


async def create_pool() -> None:
    global _pool
    _pool = await asyncpg.create_pool(
        dsn=os.environ["DATABASE_URL"],
        min_size=1,
        max_size=5,           # plenty for a low-traffic single-instance deployment
        command_timeout=10,   # seconds; fail fast rather than pile up requests
    )


async def close_pool() -> None:
    global _pool
    if _pool is not None:
        await _pool.close()
        _pool = None


def get_pool() -> asyncpg.Pool:
    if _pool is None:
        raise RuntimeError("DB pool not initialised — lifespan not running?")
    return _pool
