from __future__ import annotations

from dataclasses import asdict, dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any, Optional

import geoip2.database
import geoip2.errors

DEFAULT_CITY_DB = Path("/geoip/GeoLite2-City.mmdb")
DEFAULT_ASN_DB = Path("/geoip/GeoLite2-ASN.mmdb")


@dataclass
class GeoResult:
    """Normalized GeoIP result for storage.

    We keep it small and stable (not the full MaxMind model).
    """
    country_iso: Optional[str]
    country_name: Optional[str]
    region_name: Optional[str]
    city_name: Optional[str]
    postal_code: Optional[str]
    timezone: Optional[str]
    latitude: Optional[float]
    longitude: Optional[float]
    accuracy_radius_km: Optional[int]
    asn: Optional[int]
    as_org: Optional[str]


def _safe_str(v: Any) -> Optional[str]:
    return str(v) if v is not None else None


@lru_cache(maxsize=1)
def _city_reader() -> Optional[geoip2.database.Reader]:
    if DEFAULT_CITY_DB.exists():
        return geoip2.database.Reader(str(DEFAULT_CITY_DB))
    return None


@lru_cache(maxsize=1)
def _asn_reader() -> Optional[geoip2.database.Reader]:
    if DEFAULT_ASN_DB.exists():
        return geoip2.database.Reader(str(DEFAULT_ASN_DB))
    return None


def lookup_ip(ip: str) -> Optional[dict]:
    """Lookup an IP address in GeoLite2 databases (offline).

    Returns a dict ready to embed in your report or None if not available.
    """
    city = _city_reader()
    asn = _asn_reader()

    if city is None and asn is None:
        return None

    out = GeoResult(
        country_iso=None,
        country_name=None,
        region_name=None,
        city_name=None,
        postal_code=None,
        timezone=None,
        latitude=None,
        longitude=None,
        accuracy_radius_km=None,
        asn=None,
        as_org=None,
    )

    if city is not None:
        try:
            r = city.city(ip)
            out.country_iso = _safe_str(r.country.iso_code)
            out.country_name = _safe_str(r.country.name)
            out.city_name = _safe_str(r.city.name)

            # subdivisions may be empty
            if r.subdivisions and len(r.subdivisions) > 0:
                out.region_name = _safe_str(r.subdivisions.most_specific.name)

            out.postal_code = _safe_str(r.postal.code)
            out.timezone = _safe_str(r.location.time_zone)
            out.latitude = r.location.latitude
            out.longitude = r.location.longitude
            out.accuracy_radius_km = r.location.accuracy_radius
        except geoip2.errors.AddressNotFoundError:
            pass
        except Exception:
            # You might log this in real life.
            pass

    if asn is not None:
        try:
            r2 = asn.asn(ip)
            out.asn = int(r2.autonomous_system_number) if r2.autonomous_system_number else None
            out.as_org = _safe_str(r2.autonomous_system_organization)
        except geoip2.errors.AddressNotFoundError:
            pass
        except Exception:
            pass

    return asdict(out)
