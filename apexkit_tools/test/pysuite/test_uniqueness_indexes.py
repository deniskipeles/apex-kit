"""Unique-constraint and SQL-index behaviour, exercised over HTTP.

These assert observable behaviour only (accepted / rejected writes and query
results) so they hold for any engine build: the physical storage of uniqueness
has changed across versions, but the contract has not.
"""

from __future__ import annotations

import api


def _create(server, fresh, base, fields, **extra):
    name = fresh.name(base)
    schema = {"fields": fields, "policies": api.PUBLIC}
    schema.update(extra)
    r = server.post("/api/v1/collections", json={"name": name, "schema": schema})
    assert r.status_code in (200, 201), r.text
    return name


def test_duplicate_unique_value_rejected(server, fresh):
    name = _create(server, fresh, "uq", {"email": api.text_type(True, unique=True)})
    assert server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"email": "a@b.com"}}
    ).ok

    dup = server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"email": "a@b.com"}}
    )
    assert not dup.ok, dup.text
    assert "unique constraint" in dup.text.lower()


def test_distinct_unique_values_accepted(server, fresh):
    name = _create(server, fresh, "uq", {"email": api.text_type(True, unique=True)})
    for i in range(5):
        r = server.post(
            f"/api/v1/collections/{name}/records", json={"data": {"email": f"u{i}@b.com"}}
        )
        assert r.ok, r.text
    assert server.records(name)["total"] == 5


def test_missing_unique_value_does_not_collide(server, fresh):
    name = _create(server, fresh, "uq", {"email": api.text_type(False, unique=True)})
    # NULL is permitted many times in a unique index.
    assert server.post(f"/api/v1/collections/{name}/records", json={"data": {}}).ok
    assert server.post(f"/api/v1/collections/{name}/records", json={"data": {}}).ok
    assert server.records(name)["total"] == 2


def test_duplicate_on_update_rejected(server, fresh):
    name = _create(server, fresh, "uq", {"email": api.text_type(True, unique=True)})
    first = server.record(name, {"email": "one@b.com"})
    second = server.record(name, {"email": "two@b.com"})

    clash = server.patch(
        f"/api/v1/collections/{name}/records/{second['id']}",
        json={"data": {"email": "one@b.com"}},
    )
    assert not clash.ok, clash.text

    # The original values must be untouched by the rejected update.
    kept = api.ok(server.get(f"/api/v1/collections/{name}/records/{second['id']}"))
    assert kept["data"]["email"] == "two@b.com"
    assert api.ok(server.get(f"/api/v1/collections/{name}/records/{first['id']}"))["data"][
        "email"
    ] == "one@b.com"


def test_composite_unique_pair_enforced(server, fresh):
    name = _create(
        server,
        fresh,
        "cq",
        {"tenant": api.text_type(True), "sku": api.text_type(True)},
        composite_unique=[["tenant", "sku"]],
    )
    assert server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"tenant": "a", "sku": "1"}}
    ).ok

    dup = server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"tenant": "a", "sku": "1"}}
    )
    assert not dup.ok, dup.text

    # Varying either component is a distinct pair.
    assert server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"tenant": "a", "sku": "2"}}
    ).ok
    assert server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"tenant": "b", "sku": "1"}}
    ).ok
    assert server.records(name)["total"] == 3


def test_dropping_unique_flag_permits_duplicates(server, fresh):
    name = _create(server, fresh, "uq", {"email": api.text_type(True, unique=True)})
    assert server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"email": "a@b.com"}}
    ).ok

    # Read-modify-write the schema (preserving field uids) to remove uniqueness.
    schema = api.ok(server.get(f"/api/v1/collections/{name}"))["schema"]
    schema["fields"]["email"]["unique"] = False
    api.ok(server.patch(f"/api/v1/collections/{name}", json={"schema": schema}))

    again = server.post(
        f"/api/v1/collections/{name}/records", json={"data": {"email": "a@b.com"}}
    )
    assert again.ok, again.text


def test_sql_indexed_field_is_queryable(server, fresh):
    name = _create(
        server,
        fresh,
        "ix",
        {"status": api.text_type(False, sql_indexed=True), "qty": api.number_type()},
    )
    for i in range(6):
        server.record(name, {"status": "even" if i % 2 == 0 else "odd", "qty": i})

    body = api.ok(
        server.get(
            f"/api/v1/collections/{name}/records",
            params={"filter": '{"status":"even"}'},
        )
    )
    assert body["total"] == 3
    assert all(r["data"]["status"] == "even" for r in body["items"])


def test_index_does_not_change_result_ordering(server, fresh):
    name = _create(
        server, fresh, "ix", {"qty": api.number_type(sql_indexed=True)}
    )
    for i in range(5):
        server.record(name, {"qty": i})

    asc = api.ok(server.get(f"/api/v1/collections/{name}/records", params={"sort": "qty"}))
    assert [r["data"]["qty"] for r in asc["items"]] == [0, 1, 2, 3, 4]

    desc = api.ok(server.get(f"/api/v1/collections/{name}/records", params={"sort": "-qty"}))
    assert [r["data"]["qty"] for r in desc["items"]] == [4, 3, 2, 1, 0]
