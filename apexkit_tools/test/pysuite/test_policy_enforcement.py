"""Policy enforcement across read/write paths."""

from __future__ import annotations

import api


def test_admin_only_collection_denies_anonymous(server, fresh):
    name = fresh.make_collection("secrets", {"title": api.text_type(True)}, api.ADMIN_ONLY)
    r = server.get(f"/api/v1/collections/{name}/records", headers={"Authorization": ""})
    assert r.status_code == 403, r.text


def test_admin_only_collection_allows_admin(server, fresh):
    name = fresh.make_collection("secrets", {"title": api.text_type(True)}, api.ADMIN_ONLY)
    body = api.ok(server.get(f"/api/v1/collections/{name}/records"))
    assert body["total"] == 0


def test_public_collection_allows_anonymous_read(server, fresh):
    name = fresh.make_collection("open", {"title": api.text_type(True)}, api.PUBLIC)
    server.record(name, {"title": "visible"})
    body = api.ok(server.get(f"/api/v1/collections/{name}/records", headers={"Authorization": ""}))
    assert body["total"] == 1


def test_public_create_then_admin_delete_policy(server, fresh):
    policies = {
        "read": "public",
        "create": "public",
        "update": "admin",
        "delete": "admin",
    }
    name = fresh.make_collection("mixed", {"title": api.text_type(True)}, policies)
    created = api.ok(
        server.post(
            f"/api/v1/collections/{name}/records",
            json={"data": {"title": "anon"}},
            headers={"Authorization": ""},
        )
    )
    # Anonymous delete must be denied.
    r = server.delete(
        f"/api/v1/collections/{name}/records/{created['id']}",
        headers={"Authorization": ""},
    )
    assert r.status_code == 403, r.text
