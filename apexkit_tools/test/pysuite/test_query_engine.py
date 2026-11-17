"""Advanced query engine endpoint (/collections/{id}/query)."""

from __future__ import annotations

import api


def _seed(server, name):
    for i in range(1, 6):
        server.record(name, {"title": f"Item {i}", "qty": i})


def test_query_select_all(server, fresh):
    name = fresh.make_collection(
        "qitems", {"title": api.text_type(True), "qty": api.number_type()}
    )
    _seed(server, name)
    body = api.ok(
        server.post(f"/api/v1/collections/{name}/query", json={"select": ["*"], "limit": 10})
    )
    assert isinstance(body, list)
    assert len(body) >= 1


def test_query_group_by(server, fresh):
    name = fresh.make_collection("qitems", {"qty": api.number_type()})
    _seed(server, name)
    body = api.ok(
        server.post(
            f"/api/v1/collections/{name}/query",
            json={"select": ["qty"], "group_by": ["qty"]},
        )
    )
    assert isinstance(body, list)
    qtys = sorted(row["qty"] for row in body if "qty" in row)
    assert qtys == [1, 2, 3, 4, 5]


def test_query_limit(server, fresh):
    name = fresh.make_collection("qitems", {"qty": api.number_type()})
    _seed(server, name)
    body = api.ok(
        server.post(
            f"/api/v1/collections/{name}/query", json={"select": ["qty"], "limit": 2}
        )
    )
    assert len(body) == 2


def test_query_on_missing_collection(server):
    r = server.post("/api/v1/collections/nope/query", json={"select": ["*"]})
    assert r.status_code == 404
