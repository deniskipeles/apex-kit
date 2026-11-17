"""Concurrency helpers for the stress/benchmark tests.

Provides a bounded thread-pool burst runner and a latency recorder. These are
deliberately lightweight (stdlib only) so the suite stays dependency-free.
"""

from __future__ import annotations

import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from typing import Any, Callable

import requests


@dataclass
class Latency:
    """Accumulates latencies and status codes from a burst."""

    samples: list[float] = field(default_factory=list)
    statuses: list[int] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    _lock: threading.Lock = field(default_factory=threading.Lock)

    def record(self, seconds: float, status: int) -> None:
        with self._lock:
            self.samples.append(seconds)
            self.statuses.append(status)

    def fail(self, message: str) -> None:
        with self._lock:
            self.errors.append(message)

    @property
    def count(self) -> int:
        return len(self.samples)

    @property
    def error_rate(self) -> float:
        if not self.statuses:
            return 0.0
        bad = sum(1 for s in self.statuses if s >= 500)
        return bad / len(self.statuses)

    def percentile(self, pct: float) -> float:
        if not self.samples:
            return 0.0
        ordered = sorted(self.samples)
        k = max(0, min(len(ordered) - 1, int(round((pct / 100.0) * (len(ordered) - 1)))))
        return ordered[k]

    @property
    def mean(self) -> float:
        return sum(self.samples) / len(self.samples) if self.samples else 0.0

    @property
    def throughput(self) -> float:
        total = sum(self.samples)
        return (self.count / total) if total > 0 else 0.0


def burst(
    call: Callable[[int], requests.Response],
    total: int,
    concurrency: int = 16,
    timeout: float = 30.0,
) -> Latency:
    """Run `total` requests across `concurrency` worker threads.

    `call(i)` receives the request index and must return a requests.Response.
    """
    lat = Latency()

    def worker(i: int) -> None:
        start = time.perf_counter()
        try:
            resp = call(i)
            lat.record(time.perf_counter() - start, resp.status_code)
        except Exception as exc:  # noqa: BLE001 - surface any transport failure
            lat.record(time.perf_counter() - start, 0)
            lat.fail(f"req {i}: {exc!r}")

    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        futures = [pool.submit(worker, i) for i in range(total)]
        for _ in as_completed(futures):
            pass
    return lat


def timed(call: Callable[[], Any]) -> tuple[Any, float]:
    """Run a callable and return (result, elapsed_seconds)."""
    start = time.perf_counter()
    result = call()
    return result, time.perf_counter() - start
