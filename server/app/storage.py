"""Will change to database."""

import json
from pathlib import Path
from typing import Any

DATA_DIR = Path(__file__).resolve().parent.parent / "data"
DATA_DIR.mkdir(parents=True, exist_ok=True)
REPORTS_FILE = DATA_DIR / "reports.jsonl"

def append_report(obj: Any) -> None:
    with REPORTS_FILE.open("a", encoding="utf-8") as f:
        f.write(json.dumps(obj, ensure_ascii=False) + "\n")
