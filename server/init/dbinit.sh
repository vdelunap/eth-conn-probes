#!/bin/bash
# Database initialisation — runs once on first container start (empty volume).
# Execution order matters: roles must exist before schema.sql references them in GRANT.
set -e

psql -U "${POSTGRES_USER}" "${POSTGRES_DB}" -f /tmp/base
psql -U "${POSTGRES_USER}" "${POSTGRES_DB}" -f /var/run/secrets/auth
