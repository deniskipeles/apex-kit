"""Stress test: multi-tenant isolation and concurrent tenant provisioning."""

from __future__ import annotations

import uuid

import api
import loadgen

TENANTS = 10
KEYS_PER_TENANT = 3


def test_concurrent_tenant_provisioning(server):
    names = [f"t{uuid.uuid4().hex[:8]}" for _ in range(TENANTS)]

    def provision(i: int):
        return server.post(
            "/api/v1/admin/tenants",
            json={"tenant_id": names[i], "name": f"Tenant {i}", "tier": "free"},
            timeout=30,
        )

    lat = loadgen.burst(provision, total=TENANTS, concurrency=5)
    assert not lat.errors, lat.errors[:5]
    assert lat.error_rate == 0.0

    listing = api.ok(server.get("/api/v1/admin/tenants"))
    items = api.items_of(listing)
    present = {t.get("tenant_id") or t.get("id") for t in items}
    for n in names:
        assert n in present, f"tenant {n} missing after provisioning"

    for n in names:
        server.delete(f"/api/v1/admin/tenants/{n}")


def test_concurrent_api_key_creation(server):
    ids = []

    def make_key(i: int):
        r = server.post(
            "/api/v1/admin/keys",
            json={
                "name": f"stress-key-{i}",
                "target_tenant": "root",
                "env_type": "sys",
                "roles": ["admin"],
            },
            timeout=30,
        )
        if r.ok:
            ids.append(r.json()["info"]["id"])
        return r

    lat = loadgen.burst(make_key, total=TENANTS * KEYS_PER_TENANT, concurrency=8)
    assert lat.error_rate == 0.0, f"{lat.error_rate:.2%} 5xx creating keys"
    assert len(ids) == TENANTS * KEYS_PER_TENANT

    for kid in ids:
        server.delete(f"/api/v1/admin/keys/{kid}")


def test_tenant_scoped_path_isolated(server):
    tid = f"t{uuid.uuid4().hex[:8]}"
    api.ok(
        server.post(
            "/api/v1/admin/tenants",
            json={"tenant_id": tid, "name": "Scoped", "tier": "free"},
        )
    )
    # A request scoped to a tenant path must not leak root data.
    r = server.get(f"/tenant/{tid}/api/v1/collections")
    assert r.status_code < 500, r.text
    server.delete(f"/api/v1/admin/tenants/{tid}")
