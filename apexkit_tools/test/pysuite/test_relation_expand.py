"""Expanding relation fields through the record list endpoint.

`expand` resolves a relation field into the linked record (or records). These
assertions describe the observable shape, which is stable across the edge-only
refactor: forward expand yields the target object, reverse expand yields a list.
"""

from __future__ import annotations

import api


def _linked_pair(server, fresh, cardinality="many"):
    authors = fresh.make_collection("authors", {"name": api.text_type(True)})
    books = server.post(
        "/api/v1/collections",
        json={
            "name": fresh.name("books"),
            "schema": {
                "fields": {"title": api.text_type(True)},
                "relations": {
                    "author": {
                        "target_collection": authors,
                        "relation_type": cardinality,
                    }
                },
                "policies": api.PUBLIC,
            },
        },
    )
    assert books.status_code in (200, 201), books.text
    books_name = books.json()["name"]

    author = server.record(authors, {"name": "Jane"})
    book = server.record(books_name, {"title": "The Book", "author": author["id"]})
    return authors, books_name, author, book


def test_forward_expand_returns_the_target(server, fresh):
    _authors, books_name, author, _book = _linked_pair(server, fresh)

    body = api.ok(
        server.get(f"/api/v1/collections/{books_name}/records", params={"expand": "author"})
    )
    items = api.items_of(body)
    assert len(items) == 1

    expanded = items[0]["expand"]["author"]
    assert expanded["id"] == author["id"]
    assert expanded["data"]["name"] == "Jane"


def test_reverse_expand_returns_a_list(server, fresh):
    authors, books_name, author, _book = _linked_pair(server, fresh)

    body = api.ok(
        server.get(f"/api/v1/collections/{authors}/records", params={"expand": books_name})
    )
    items = api.items_of(body)
    assert len(items) == 1

    children = items[0]["expand"][books_name]
    assert isinstance(children, list)
    assert len(children) == 1
    assert children[0]["data"]["title"] == "The Book"


def test_expanding_after_target_delete_yields_no_link(server, fresh):
    authors, books_name, author, _book = _linked_pair(server, fresh)

    deleted = server.delete(f"/api/v1/collections/{authors}/records/{author['id']}")
    assert deleted.status_code in (200, 204), deleted.text

    body = api.ok(
        server.get(f"/api/v1/collections/{books_name}/records", params={"expand": "author"})
    )
    items = api.items_of(body)
    # The dangling link must not resolve to the removed record.
    assert items[0]["expand"]["author"] is None
