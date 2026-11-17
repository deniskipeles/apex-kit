"""Vector search routes backed by the vector engine."""

from __future__ import annotations

import api


def _vector_collection(server, fresh):
    return fresh.make_collection(
        "vecs",
        {
            "title": api.text_type(True),
            "embedding": {
                "type": "vector",
                "required": False,
                "vectorize": False,
                "dimension": 3,
            },
        },
    )


def test_create_record_with_vector(server, fresh):
    name = _vector_collection(server, fresh)
    rec = server.record(name, {"title": "A", "embedding": [1.0, 0.0, 0.0]})
    assert rec["data"]["embedding"] == [1.0, 0.0, 0.0]


def test_search_vector_with_vector(server, fresh):
    name = _vector_collection(server, fresh)
    server.record(name, {"title": "A", "embedding": [1.0, 0.0, 0.0]})
    body = api.ok(
        server.post(
            f"/api/v1/collections/{name}/search-vector-with-vector",
            json={"field": "embedding", "vector": [0.9, 0.1, 0.0], "limit": 5},
        )
    )
    assert "items" in body
    assert "total" in body


def test_search_vector_with_text_requires_query_text(server, fresh):
    name = _vector_collection(server, fresh)
    r = server.post(
        f"/api/v1/collections/{name}/search-vector-with-text",
        json={"field": "embedding", "limit": 5},
    )
    assert r.status_code == 422


def test_get_record_vector(server, fresh):
    name = _vector_collection(server, fresh)
    rec = server.record(name, {"title": "A", "embedding": [0.5, 0.5, 0.0]})
    body = api.ok(
        server.get(f"/api/v1/collections/{name}/get-vector/{rec['id']}")
    )
    assert isinstance(body, list)
