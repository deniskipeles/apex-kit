"""pytest configuration and fixtures for the ApexKit Python suite.

Provides a session-scoped, authenticated ApexKit server plus per-test helpers.
Never builds anything: the binary is supplied via ``--bin``, ``APEXKIT_BIN``, or
a default location.
"""

from __future__ import annotations

import pytest

from harness import ApexServer, find_binary


def pytest_addoption(parser) -> None:
    parser.addoption(
        "--bin",
        action="store",
        default=None,
        help="Path to the prebuilt ApexKit binary (this suite never builds).",
    )


@pytest.fixture(scope="session")
def binary_path(pytestconfig) -> str | None:
    return pytestconfig.getoption("--bin") or find_binary()


@pytest.fixture(scope="session")
def server(binary_path):
    """A single authenticated ApexKit instance shared by the whole session."""
    if not binary_path:
        pytest.skip("no ApexKit binary available (set APEXKIT_BIN or --bin)")
    srv = ApexServer(binary=binary_path)
    srv.start()
    srv.raise_rate_limits()
    try:
        yield srv
    finally:
        srv.stop()


@pytest.fixture()
def fresh(server):
    """Per-test isolation: a uniquely-named namespace derived from the test id.

    Collections created through the returned helper get a unique prefix so
    tests never collide on the shared server.
    """
    import uuid

    prefix = f"t{uuid.uuid4().hex[:10]}"

    class Namespaced:
        def __init__(self, srv: ApexServer, pfx: str) -> None:
            self.srv = srv
            self.prefix = pfx
            self._names: list[str] = []

        def name(self, base: str) -> str:
            n = f"{self.prefix}_{base}"
            self._names.append(n)
            return n

        def collection(self, base: str, fields: dict, policies: dict | None = None) -> str:
            return self.name(base)

        def make_collection(self, base: str, fields: dict, policies: dict | None = None) -> str:
            name = self.name(base)
            self.srv.collection(name, fields, policies)
            return name

    return Namespaced(server, prefix)
