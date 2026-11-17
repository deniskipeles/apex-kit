"""Record relations: create and delete links between records."""

from __future__ import annotations

import api


def _two_collections(server, fresh):
    books = fresh.make_collection("books", {"title": api.text_type(True)})
    authors = fresh.make_collection("authors", {"name": api.text_type(True)})
    book = server.record(books, {"title": "The Book"})
    author = server.record(authors, {"name": "Jane"})
    author_col = api.ok(server.get(f"/api/v1/collections/{authors}"))
    return books, authors, book, author, author_col["id"]


def test_create_relation(server, fresh):
    books, authors, book, author, author_id = _two_collections(server, fresh)
    r = server.post(
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_id,
            "target_record_id": author["id"],
            "relation_name": "author",
        },
    )
    assert r.status_code in (200, 201, 204), r.text


def test_delete_relation(server, fresh):
    books, authors, book, author, author_id = _two_collections(server, fresh)
    server.post(
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_id,
            "target_record_id": author["id"],
            "relation_name": "author",
        },
    )
    r = server.request(
        "DELETE",
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_id,
            "target_record_id": author["id"],
            "relation_name": "author",
        },
    )
    assert r.status_code in (200, 204, 404)


def test_relation_to_missing_record_still_responds(server, fresh):
    books, authors, book, _author, author_id = _two_collections(server, fresh)
    r = server.post(
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_id,
            "target_record_id": 999999,
            "relation_name": "author",
        },
    )
    # Must not 5xx; either accepts or rejects cleanly.
    assert r.status_code < 500, r.text
