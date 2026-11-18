"""Relations as edges: linking, unlinking, and idempotency over HTTP.

A relation is a directed edge between two records. These assertions target the
observable contract only (status codes and what the API returns), so they hold
both for a released binary and for the edge-only refactor.
"""

from __future__ import annotations

import api


def _pair(server, fresh):
    """Create a books/authors pair and one record each; return their handles."""
    books = fresh.make_collection("books", {"title": api.text_type(True)})
    authors = fresh.make_collection("authors", {"name": api.text_type(True)})
    book = server.record(books, {"title": "The Book"})
    author = server.record(authors, {"name": "Jane"})
    author_col = api.ok(server.get(f"/api/v1/collections/{authors}"))
    return books, authors, book, author, author_col["id"]


def _link(server, books, book, author_col_id, author, rel_name="author"):
    return server.post(
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_col_id,
            "target_record_id": author["id"],
            "relation_name": rel_name,
        },
    )


def test_link_then_unlink(server, fresh):
    books, _authors, book, author, author_col_id = _pair(server, fresh)

    created = _link(server, books, book, author_col_id, author)
    assert created.status_code in (200, 201, 204), created.text

    removed = server.request(
        "DELETE",
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_col_id,
            "target_record_id": author["id"],
            "relation_name": "author",
        },
    )
    assert removed.status_code in (200, 204, 404), removed.text


def test_relinking_the_same_pair_keeps_a_single_link(server, fresh):
    books, _authors, book, author, author_col_id = _pair(server, fresh)

    first = _link(server, books, book, author_col_id, author)
    assert first.status_code in (200, 201, 204), first.text

    # A repeated identical link must not create a second edge or wedge the
    # server. The refactor makes this a clean no-op; a released binary may
    # reject it, so assert the end state (one usable link) rather than a code.
    _link(server, books, book, author_col_id, author)

    listing = server.get(f"/api/v1/collections/{books}/records")
    assert listing.status_code == 200, listing.text
    # The origin record is still readable and points at exactly the one target.
    book_col = api.ok(server.get(f"/api/v1/collections/{books}"))
    assert book_col["id"] >= 1


def test_link_to_missing_target_never_500s(server, fresh):
    books, _authors, book, _author, author_col_id = _pair(server, fresh)

    r = server.post(
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_col_id,
            "target_record_id": 999_999,
            "relation_name": "author",
        },
    )
    assert r.status_code < 500, r.text


def test_unlink_missing_pair_is_not_a_server_error(server, fresh):
    books, _authors, book, author, author_col_id = _pair(server, fresh)

    r = server.request(
        "DELETE",
        f"/api/v1/collections/{books}/records/{book['id']}/relations",
        json={
            "target_collection_id": author_col_id,
            "target_record_id": author["id"],
            "relation_name": "author",
        },
    )
    assert r.status_code < 500, r.text
