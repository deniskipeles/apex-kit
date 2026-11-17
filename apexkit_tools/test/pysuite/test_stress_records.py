"""Stress test: concurrent record writes and reads against one collection.

Hammer the record CRUD path with a bounded thread pool and assert the engine
stays consistent (no 5xx, no lost writes) under contention.
"""

from __future__ import annotations

import api
import loadgen

WRITERS = 32
PER_WRITER = 10
READERS = 16
PER_READER = 25


def _collection(server, fresh):
    return fresh.make_collection(
        "stress_records",
        {"title": api.text_type(True), "qty": api.number_type()},
    )


def test_concurrent_writes_all_persist(server, fresh):
    name = _collection(server, fresh)
    total = WRITERS * PER_WRITER

    def do_write(i: int):
        return server.post(
            f"/api/v1/collections/{name}/records",
            json={"data": {"title": f"w{i}", "qty": i}},
            timeout=30,
        )

    lat = loadgen.burst(do_write, total=total, concurrency=WRITERS)
    assert not lat.errors, lat.errors[:5]
    assert lat.error_rate == 0.0, f"{lat.error_rate:.2%} of writes returned 5xx"

    listing = api.ok(server.get(f"/api/v1/collections/{name}/records?per_page=1"))
    assert listing["total"] == total, f"expected {total} records, got {listing['total']}"


def test_concurrent_reads_stable(server, fresh):
    name = _collection(server, fresh)
    for i in range(20):
        server.record(name, {"title": f"seed{i}", "qty": i})

    def do_read(_i: int):
        return server.get(f"/api/v1/collections/{name}/records?per_page=5", timeout=30)

    lat = loadgen.burst(do_read, total=READERS * PER_READER, concurrency=READERS)
    assert lat.error_rate == 0.0, f"{lat.error_rate:.2%} of reads returned 5xx"
    assert lat.count == READERS * PER_READER


def test_mixed_read_write_contention(server, fresh):
    name = _collection(server, fresh)

    def do_mixed(i: int):
        if i % 3 == 0:
            return server.get(f"/api/v1/collections/{name}/records?per_page=10", timeout=30)
        return server.post(
            f"/api/v1/collections/{name}/records",
            json={"data": {"title": f"mix{i}", "qty": i}},
            timeout=30,
        )

    lat = loadgen.burst(do_mixed, total=180, concurrency=24)
    assert lat.error_rate == 0.0, f"{lat.error_rate:.2%} 5xx under mixed load"
    # All non-read requests (2/3 of 180) should have created a record.
    expected = sum(1 for i in range(180) if i % 3 != 0)
    listing = api.ok(server.get(f"/api/v1/collections/{name}/records?per_page=1"))
    assert listing["total"] == expected
