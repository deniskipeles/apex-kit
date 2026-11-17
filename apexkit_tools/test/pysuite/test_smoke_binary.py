"""Smoke test: the binary boots, serves, and shuts down cleanly."""

from __future__ import annotations

import subprocess

import harness


def test_binary_is_executable(binary_path):
    if not binary_path:
        import pytest

        pytest.skip("no binary available")
    import os

    assert os.access(binary_path, os.X_OK)


def test_binary_version_flag(binary_path):
    if not binary_path:
        import pytest

        pytest.skip("no binary available")
    out = subprocess.run(
        [binary_path, "--version"], capture_output=True, text=True, timeout=15
    )
    assert out.returncode == 0
    assert out.stdout.strip()


def test_binary_help_flag(binary_path):
    if not binary_path:
        import pytest

        pytest.skip("no binary available")
    out = subprocess.run(
        [binary_path, "--help"], capture_output=True, text=True, timeout=15
    )
    assert out.returncode == 0
    assert "ApexKit CLI & Server Entrypoint" in out.stdout


def test_server_boots_and_reports_health(binary_path):
    if not binary_path:
        import pytest

        pytest.skip("no binary available")
    srv = harness.ApexServer(binary=binary_path)
    try:
        srv.start()
        assert srv.token
        body = srv.get("/version").json()
        assert "vector" in body
    finally:
        srv.stop()
