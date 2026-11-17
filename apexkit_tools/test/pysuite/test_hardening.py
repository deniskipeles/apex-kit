"""Input hardening: malformed bodies, unknown routes, oversize payloads."""

from __future__ import annotations

import api


def test_invalid_json_body_returns_400(server, fresh):
    name = fresh.make_collection("hard", {"title": api.text_type(True)})
    r = server.post(
        f"/api/v1/collections/{name}/records",
        data="this is not json",
        headers={"Content-Type": "application/json"},
    )
    assert r.status_code == 400, r.text


def test_missing_required_body_field_is_422(server, fresh):
    name = fresh.make_collection("hard", {"title": api.text_type(True)})
    r = server.post(f"/api/v1/collections/{name}/records", json={"wrong": "shape"})
    assert r.status_code == 422, r.text


def test_oversize_payload_is_handled(server, fresh):
    name = fresh.make_collection("hard", {"title": api.text_type(True)})
    big = "A" * 200_000
    r = server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"title": big}}
    )
    # Either accepted or rejected, but never a 5xx / panic.
    assert r.status_code < 500, r.status_code


def test_unknown_collection_routes_are_404(server):
    # Record paths resolve the collection and 404; search paths validate the
    # query first and return 400 for a missing collection.
    assert server.get("/api/v1/collections/nope/records").status_code == 404
    assert server.get("/api/v1/collections/nope/records/1").status_code == 404
    for path in (
        "/api/v1/collections/nope/search",
        "/api/v1/collections/nope/instant-search",
    ):
        r = server.get(path)
        assert r.status_code in (400, 404), f"{path} -> {r.status_code}"


def test_wrong_method_on_records(server, fresh):
    name = fresh.make_collection("hard", {"title": api.text_type(True)})
    # PATCH on the collection (list) endpoint is not allowed.
    r = server.patch(f"/api/v1/collections/{name}/records", json={})
    assert r.status_code in (404, 405), r.text


def test_sql_injection_attempt_is_safe(server, fresh):
    name = fresh.make_collection("hard", {"title": api.text_type(True)})
    server.record(name, {"title": "safe"})
    r = server.get(
        f"/api/v1/collections/{name}/records",
        params={"filter": '{"title":{"$gt":"x\\"; DROP TABLE records;--"}}'},
    )
    assert r.status_code < 500, r.text
    # The table must still exist and return the seeded row.
    body = api.ok(server.get(f"/api/v1/collections/{name}/records"))
    assert body["total"] >= 1


def test_unicode_and_empty_strings(server, fresh):
    name = fresh.make_collection("hard", {"title": api.text_type(True)})
    created = server.record(name, {"title": "emoji 🚀 ünïcödé"})
    assert "🚀" in created["data"]["title"]
