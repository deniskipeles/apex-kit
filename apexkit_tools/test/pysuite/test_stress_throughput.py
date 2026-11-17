"""Stress test: sustained read throughput with latency assertions."""

from __future__ import annotations

import api
import loadgen

ROUNDS = 300
CONCURRENCY = 24


def test_sustained_read_throughput(server, fresh):
    name = fresh.make_collection(
        "bench_read", {"title": api.text_type(True), "qty": api.number_type()}
    )
    for i in range(50):
        server.record(name, {"title": f"row{i}", "qty": i})

    def do_read(i: int):
        return server.get(
            f"/api/v1/collections/{name}/records?per_page=10&page={(i % 5) + 1}",
            timeout=30,
        )

    lat = loadgen.burst(do_read, total=ROUNDS, concurrency=CONCURRENCY)
    assert lat.error_rate == 0.0, f"{lat.error_rate:.2%} 5xx under sustained reads"
    assert lat.count == ROUNDS
    # Sanity floor: the engine must complete the burst, not hang.
    assert lat.percentile(50) < 5.0, f"p50 too slow: {lat.percentile(50):.3f}s"


def test_sustained_filter_queries(server, fresh):
    name = fresh.make_collection("bench_filter", {"qty": api.number_type()})
    for i in range(100):
        server.record(name, {"qty": i})

    def do_query(i: int):
        return server.get(
            f"/api/v1/collections/{name}/records",
            params={"filter": '{"qty":{"$gt":%d}}' % (i % 50), "limit": 20},
            timeout=30,
        )

    lat = loadgen.burst(do_query, total=200, concurrency=16)
    assert lat.error_rate == 0.0, f"{lat.error_rate:.2%} 5xx under filter load"


def test_health_endpoint_under_load(server):
    def do_health(_i: int):
        return server.get("/healthz", timeout=10)

    lat = loadgen.burst(do_health, total=400, concurrency=32)
    assert lat.error_rate == 0.0
    assert all(s == 200 for s in lat.statuses)
