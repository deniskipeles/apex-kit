"""Benchmark: baseline latency and throughput measurements.

These are measurement tests, not pass/fail performance gates beyond generous
sanity floors, so they remain stable on shared CI machines.
"""

from __future__ import annotations

import api
import loadgen


def test_health_latency_baseline(server):
    lat = loadgen.burst(lambda _i: server.get("/healthz", timeout=10), total=200, concurrency=16)
    assert lat.error_rate == 0.0
    p50, p95 = lat.percentile(50), lat.percentile(95)
    assert p50 < 2.0, f"health p50 {p50:.4f}s"
    assert p95 < 5.0, f"health p95 {p95:.4f}s"


def test_create_record_latency(server, fresh):
    name = fresh.make_collection("bench_create", {"title": api.text_type(True)})

    def do_create(i: int):
        return server.post(
            f"/api/v1/collections/{name}/records",
            json={"data": {"title": f"b{i}"}},
            timeout=30,
        )

    lat = loadgen.burst(do_create, total=100, concurrency=8)
    assert lat.error_rate == 0.0
    assert lat.percentile(50) < 5.0, f"create p50 {lat.percentile(50):.4f}s"


def test_list_record_latency(server, fresh):
    name = fresh.make_collection("bench_list", {"title": api.text_type(True)})
    for i in range(50):
        server.record(name, {"title": f"r{i}"})

    def do_list(_i: int):
        return server.get(f"/api/v1/collections/{name}/records?per_page=25", timeout=30)

    lat = loadgen.burst(do_list, total=150, concurrency=12)
    assert lat.error_rate == 0.0
    assert lat.percentile(95) < 5.0, f"list p95 {lat.percentile(95):.4f}s"


def test_throughput_is_positive(server, fresh):
    name = fresh.make_collection("bench_tp", {"title": api.text_type(True)})
    server.record(name, {"title": "x"})
    lat = loadgen.burst(
        lambda _i: server.get(f"/api/v1/collections/{name}/records", timeout=30),
        total=100,
        concurrency=16,
    )
    assert lat.throughput > 0.0, "measured zero throughput"
