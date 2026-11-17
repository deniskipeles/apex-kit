"""CLI subcommand behaviour, driven against the prebuilt binary."""

from __future__ import annotations

import subprocess

import pytest


def run(binary, args, cwd=None):
    return subprocess.run(
        [binary, *args], capture_output=True, text=True, timeout=30, cwd=cwd
    )


@pytest.fixture()
def binary(binary_path):
    if not binary_path:
        pytest.skip("no binary available")
    return binary_path


def test_user_help(binary):
    out = run(binary, ["user", "--help"])
    assert out.returncode == 0
    for sub in ("create", "list", "reset-password"):
        assert sub in out.stdout


def test_data_help(binary):
    out = run(binary, ["data", "--help"])
    assert out.returncode == 0
    for sub in ("reindex", "export", "prune-logs"):
        assert sub in out.stdout


def test_wasm_help(binary):
    out = run(binary, ["wasm", "--help"])
    assert out.returncode == 0
    assert "--get" in out.stdout and "--file" in out.stdout


def test_backup_help(binary):
    out = run(binary, ["backup", "--help"])
    assert out.returncode == 0
    assert "--out" in out.stdout


def test_restore_requires_file(binary):
    out = run(binary, ["restore"])
    assert out.returncode != 0


def test_unknown_subcommand_fails(binary):
    out = run(binary, ["definitely-not-a-command"])
    assert out.returncode != 0
