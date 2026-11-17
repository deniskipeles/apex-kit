"""Admin routes: users, settings, tenants, API keys, sandboxes, logs, backups."""

from __future__ import annotations

import uuid

import api


def test_list_users(server):
    body = api.ok(server.get("/api/v1/admin/users"))
    items = api.items_of(body)
    assert any(u["email"] == "admin@apexkit.io" for u in items)


def test_get_settings(server):
    body = api.ok(server.get("/api/v1/admin/settings"))
    assert "allow_public_registration" in body
    assert "security" in body


def test_update_settings_round_trip(server):
    api.ok(server.patch("/api/v1/admin/settings", json={"app_name": "PytestKit"}))
    body = api.ok(server.get("/api/v1/admin/settings"))
    assert body["app_name"] == "PytestKit"


def test_dashboard_stats_shape(server):
    body = api.ok(server.get("/api/v1/admin/dashboard"))
    assert "stats" in body
    for key in ("collections_count", "total_records", "total_vectors"):
        assert key in body["stats"]


def test_tenant_lifecycle(server):
    tid = f"t{uuid.uuid4().hex[:8]}"
    created = api.ok(
        server.post(
            "/api/v1/admin/tenants",
            json={"tenant_id": tid, "name": "Pytest Tenant", "tier": "free"},
        ),
        *(range(200, 300)),
    )
    assert created.get("tenant_id") == tid

    listing = api.ok(server.get("/api/v1/admin/tenants"))
    items = api.items_of(listing)
    assert any(t.get("tenant_id") == tid or t.get("id") == tid for t in items)

    r = server.delete(f"/api/v1/admin/tenants/{tid}")
    assert r.status_code in (200, 204)


def test_api_key_lifecycle(server):
    created = api.ok(
        server.post(
            "/api/v1/admin/keys",
            json={
                "name": "pytest-key",
                "target_tenant": "root",
                "env_type": "sys",
                "roles": ["admin"],
            },
        )
    )
    assert created["key"]
    key_id = created["info"]["id"]

    listing = api.ok(server.get("/api/v1/admin/keys"))
    items = api.items_of(listing)
    assert any(k["id"] == key_id for k in items)

    r = server.delete(f"/api/v1/admin/keys/{key_id}")
    assert r.status_code in (200, 204)


def test_sandbox_lifecycle(server):
    created = api.ok(
        server.post(
            "/api/v1/admin/sandboxes",
            json={"name": "pytest-sb", "clone_strategy": "none"},
        )
    )
    assert created["id"]
    r = server.delete(f"/api/v1/admin/sandboxes/{created['id']}")
    assert r.status_code in (200, 204)


def test_audit_logs_shape(server):
    body = api.ok(server.get("/api/v1/admin/logs"))
    assert "items" in body
    assert "total" in body


def test_list_backups(server):
    body = api.ok(server.get("/api/v1/admin/backups"))
    assert isinstance(body, list)


def test_admin_routes_require_auth(server):
    r = server.get("/api/v1/admin/users", headers={"Authorization": ""})
    assert r.status_code in (401, 403)
