"""Rate limiting middleware behaviour."""

from __future__ import annotations

import loadgen


def test_rate_limit_headers_present(server):
    r = server.get("/healthz")
    # Health may bypass limiting; fall back to a normal API route.
    if "x-ratelimit-limit" not in r.headers:
        r = server.get("/api/v1/auth/me")
    assert "x-ratelimit-limit" in r.headers, dict(r.headers)


def test_admin_bypasses_tight_limit(server):
    # Admin/API requests carry a very high limit in this suite; a small burst
    # must never be throttled.
    lat = loadgen.burst(
        lambda _i: server.get("/api/v1/auth/me", timeout=10),
        total=50,
        concurrency=10,
    )
    assert all(s in (200, 401) for s in lat.statuses), lat.statuses
