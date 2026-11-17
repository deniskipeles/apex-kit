"""Collection CRUD, schema round-tripping, and validation."""

from __future__ import annotations

import api


def test_create_and_get_collection(server, fresh):
    name = fresh.make_collection("posts", {"title": api.text_type(True)})
    body = api.ok(server.get(f"/api/v1/collections/{name}"))
    assert body["name"] == name
    assert "title" in body["schema"]["fields"]


def test_list_collections_includes_created(server, fresh):
    name = fresh.make_collection("things", {"label": api.text_type(True)})
    body = api.ok(server.get("/api/v1/collections"))
    items = api.items_of(body)
    assert any(c.get("name") == name for c in items)


def test_create_collection_invalid_name_rejected(server):
    r = server.post(
        "/api/v1/collections",
        json={"name": "bad name!!", "schema": None},
    )
    assert not r.ok
    assert r.status_code in (400, 422, 500)


def test_create_duplicate_collection_is_tolerated(server, fresh):
    name = fresh.make_collection("dup", {"a": api.text_type()})
    r = server.post(
        "/api/v1/collections",
        json={
            "name": name,
            "schema": {
                "fields": {"a": api.text_type()},
                "policies": api.PUBLIC,
            },
        },
    )
    # Names are not enforced unique in this engine; a repeat create must not
    # 5xx and must still return a valid id.
    assert r.status_code in (200, 201), r.text
    assert r.json()["id"]


def test_update_collection_renames(server, fresh):
    name = fresh.make_collection("oldname", {"a": api.text_type()})
    new_name = fresh.name("newname")
    body = api.ok(
        server.patch(f"/api/v1/collections/{name}", json={"name": new_name})
    )
    assert body.get("name", new_name) == new_name
    api.ok(server.get(f"/api/v1/collections/{new_name}"))


def test_delete_collection(server, fresh):
    name = fresh.make_collection("todelete", {"a": api.text_type()})
    r = server.delete(f"/api/v1/collections/{name}")
    assert r.status_code in (200, 204)
    assert server.get(f"/api/v1/collections/{name}").status_code == 404


def test_get_missing_collection_is_404(server):
    r = server.get("/api/v1/collections/nope_missing")
    assert r.status_code == 404


def test_schema_field_types_round_trip(server, fresh):
    fields = {
        "title": api.text_type(True),
        "views": api.number_type(),
        "published": api.bool_type(),
    }
    name = fresh.make_collection("typed", fields)
    body = api.ok(server.get(f"/api/v1/collections/{name}"))
    got = body["schema"]["fields"]
    assert got["title"]["type"] == "text"
    assert got["views"]["type"] == "number"
    assert got["published"]["type"] == "boolean"
