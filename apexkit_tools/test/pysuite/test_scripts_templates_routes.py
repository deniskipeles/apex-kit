"""Scripts, templates, and webhook execution routes."""

from __future__ import annotations

import uuid

import api

SCRIPT_BODY = {
    "name": "hook",
    "code": "export default async function(req) { return new Response({ ok: true }); }",
    "trigger_type": "manual",
    "active": True,
}


def test_create_and_list_scripts(server):
    name = f"hook-{uuid.uuid4().hex[:8]}"
    body = dict(SCRIPT_BODY, name=name)
    r = server.post("/api/v1/admin/scripts", json=body)
    assert r.ok, r.text

    listing = api.ok(server.get("/api/v1/admin/scripts"))
    # Listing groups local/global; flatten.
    all_scripts = []
    if isinstance(listing, dict):
        for group in listing.values():
            if isinstance(group, list):
                all_scripts.extend(group)
    else:
        all_scripts = listing
    assert any(s.get("name") == name for s in all_scripts)


def test_webhook_runs_script(server):
    name = f"wh-{uuid.uuid4().hex[:8]}"
    api.ok(server.post("/api/v1/admin/scripts", json=dict(SCRIPT_BODY, name=name)))
    r = server.post(f"/api/v1/webhook/{name}", json={})
    assert r.status_code < 500, r.text


def test_webhook_unknown_script_is_404(server):
    r = server.post(f"/api/v1/webhook/nope-{uuid.uuid4().hex}", json={})
    assert r.status_code == 404


def test_create_and_list_templates(server):
    slug = f"tpl-{uuid.uuid4().hex[:8]}"
    api.ok(
        server.post(
            "/api/v1/admin/templates",
            json={"name": slug, "slug": slug, "content": "<h1>{{title}}</h1>"},
        )
    )
    listing = api.ok(server.get("/api/v1/admin/templates"))
    assert isinstance(listing, list)
    assert any(t.get("slug") == slug for t in listing)


def test_update_template(server):
    slug = f"tpl-{uuid.uuid4().hex[:8]}"
    created = api.ok(
        server.post(
            "/api/v1/admin/templates",
            json={"name": slug, "slug": slug, "content": "old"},
        )
    )
    tid = created["id"]
    r = server.patch(
        f"/api/v1/admin/templates/{tid}", json={"content": "<p>new</p>"}
    )
    assert r.ok, r.text
    listing = api.ok(server.get("/api/v1/admin/templates"))
    match = [t for t in listing if t["id"] == tid]
    assert match and match[0]["content"] == "<p>new</p>"


def test_delete_template(server):
    slug = f"tpl-{uuid.uuid4().hex[:8]}"
    created = api.ok(
        server.post(
            "/api/v1/admin/templates",
            json={"name": slug, "slug": slug, "content": "x"},
        )
    )
    r = server.delete(f"/api/v1/admin/templates/{created['id']}")
    assert r.status_code in (200, 204)
