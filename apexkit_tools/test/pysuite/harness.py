"""Shared bootstrap harness for the ApexKit Python integration/stress suite.

The suite drives a *prebuilt* ApexKit binary. It never invokes cargo. Point it
at a binary with ``--bin`` (or the ``APEXKIT_BIN`` env var, or a default of
``./apexkit``); the harness copies it into an isolated temp workspace, boots it
on a free port, and hands back an authenticated session.

Usage as a library:

    with ApexServer() as server:
        server.collection("posts", {"title": {"type": "text", "required": True}})
        server.post("/api/v1/collections/posts/records", json={"data": {"title": "x"}})
"""

from __future__ import annotations

import base64
import contextlib
import json
import os
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Iterable

import requests

ADMIN_EMAIL = "admin@apexkit.io"
ADMIN_PASSWORD = "password"


def find_binary() -> str | None:
    """Locate the ApexKit binary using env vars and common build locations."""
    candidates = [
        os.environ.get("APEXKIT_BIN"),
        "./apexkit",
        "./target/release/apexkit-cli",
        "./target/release/apexkit-api",
        "./target/release/apexkit",
    ]
    for cand in candidates:
        if not cand:
            continue
        p = Path(cand)
        if p.is_file() and os.access(p, os.X_OK):
            return str(p.resolve())
    return None


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class ApexServer:
    """Boots and manages one isolated ApexKit instance."""

    def __init__(
        self,
        binary: str | None = None,
        port: int | None = None,
        boot_timeout: float = 30.0,
        env: dict[str, str] | None = None,
    ) -> None:
        self.binary = binary or find_binary()
        if not self.binary:
            raise RuntimeError(
                "No ApexKit binary found. Pass --bin, set APEXKIT_BIN, or place "
                "an executable at ./apexkit"
            )
        self.port = port or free_port()
        self.boot_timeout = boot_timeout
        self.workdir = Path(tempfile.mkdtemp(prefix="apexkit_pysuite_"))
        self.process: subprocess.Popen | None = None
        self.session = requests.Session()
        self.token: str | None = None
        self.master_key: str | None = None
        self._extra_env = env or {}
        self.log_path = self.workdir / "engine.log"

    # -- lifecycle ---------------------------------------------------------
    def __enter__(self) -> "ApexServer":
        self.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def start(self) -> None:
        exe = self.workdir / "apexkit"
        shutil.copy2(self.binary, exe)
        exe.chmod(0o755)

        env = os.environ.copy()
        env["PORT"] = str(self.port)
        self.master_key = base64.b64encode(secrets.token_bytes(32)).decode()
        env["APEXKIT_MASTER_KEY"] = self.master_key
        env["RUST_LOG"] = "warn"
        env.update(self._extra_env)

        log = open(self.log_path, "w")
        self.process = subprocess.Popen(
            [str(exe)],
            cwd=str(self.workdir),
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        self._wait_ready()
        self.login()

    def _wait_ready(self) -> None:
        deadline = time.time() + self.boot_timeout
        while time.time() < deadline:
            if self.process and self.process.poll() is not None:
                raise RuntimeError(
                    f"ApexKit exited early (code {self.process.returncode}).\n"
                    f"{self.read_log()[-2000:]}"
                )
            try:
                r = self.session.get(f"{self.base_url}/healthz", timeout=1)
                if r.status_code == 200:
                    return
            except requests.RequestException:
                pass
            time.sleep(0.25)
        raise RuntimeError(
            f"ApexKit did not become ready within {self.boot_timeout}s.\n"
            f"{self.read_log()[-2000:]}"
        )

    def stop(self) -> None:
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
        shutil.rmtree(self.workdir, ignore_errors=True)

    def read_log(self) -> str:
        try:
            return self.log_path.read_text(errors="replace")
        except OSError:
            return ""

    # -- auth --------------------------------------------------------------
    def login(self, email: str = ADMIN_EMAIL, password: str = ADMIN_PASSWORD) -> str:
        r = self.session.post(
            f"{self.base_url}/api/v1/auth/login",
            json={"email": email, "password": password},
            timeout=5,
        )
        r.raise_for_status()
        self.token = r.json()["token"]
        self.session.headers.update({"Authorization": f"Bearer {self.token}"})
        return self.token

    def register(self, email: str, password: str, role: str = "user") -> requests.Response:
        """Register a new user as an *anonymous* caller (no admin session)."""
        return requests.post(
            f"{self.base_url}/api/v1/auth/register",
            json={"email": email, "password": password, "role": role},
            timeout=5,
        )

    def enable_public_registration(self, enabled: bool = True) -> None:
        r = self.patch(
            "/api/v1/admin/settings",
            json={"allow_public_registration": enabled},
        )
        r.raise_for_status()

    def raise_rate_limits(self, limit: int = 100000) -> None:
        self.patch(
            "/api/v1/admin/settings",
            json={
                "security": {
                    "cors_allow_all": True,
                    "cors_origins": "",
                    "tenant_transparency": False,
                    "global_rate_limit": limit,
                    "tenant_free_rate_limit": limit,
                    "tenant_pro_rate_limit": limit,
                }
            },
        )

    # -- raw HTTP helpers --------------------------------------------------
    def request(self, method: str, path: str, **kw: Any) -> requests.Response:
        kw.setdefault("timeout", 30)
        return self.session.request(method, f"{self.base_url}{path}", **kw)

    def get(self, path: str, **kw: Any) -> requests.Response:
        return self.request("GET", path, **kw)

    def post(self, path: str, **kw: Any) -> requests.Response:
        return self.request("POST", path, **kw)

    def patch(self, path: str, **kw: Any) -> requests.Response:
        return self.request("PATCH", path, **kw)

    def put(self, path: str, **kw: Any) -> requests.Response:
        return self.request("PUT", path, **kw)

    def delete(self, path: str, **kw: Any) -> requests.Response:
        return self.request("DELETE", path, **kw)

    def graphql(self, query: str, variables: dict | None = None) -> dict:
        r = self.post(
            "/graphql",
            json={"query": query, "variables": variables or {}},
        )
        r.raise_for_status()
        return r.json()

    def master_headers(self) -> dict:
        """Headers accepted by the isolated replication router."""
        return {
            "Authorization": f"Bearer {self.master_key}",
            "Content-Type": "application/json",
        }

    # -- domain helpers ----------------------------------------------------
    def collection(self, name: str, fields: dict[str, dict], policies: dict | None = None) -> int:
        """Create a collection idempotently; returns its numeric id."""
        schema = {
            "fields": fields,
            "policies": policies
            or {
                "read": "public",
                "create": "public",
                "update": "public",
                "delete": "public",
            },
        }
        r = self.post("/api/v1/collections", json={"name": name, "schema": schema})
        if r.status_code == 201 or r.status_code == 200:
            return r.json()["id"]
        # Already exists: look it up.
        listing = self.get("/api/v1/collections").json()
        items = listing.get("items", listing if isinstance(listing, list) else [])
        for c in items:
            if c.get("name") == name:
                return c["id"]
        raise AssertionError(f"could not create collection {name}: {r.status_code} {r.text}")

    def record(self, collection: str, data: dict) -> dict:
        r = self.post(f"/api/v1/collections/{collection}/records", json={"data": data})
        r.raise_for_status()
        return r.json()

    def records(self, collection: str, **params: Any) -> dict:
        r = self.get(f"/api/v1/collections/{collection}/records", params=params)
        r.raise_for_status()
        return r.json()


def percentile(values: Iterable[float], pct: float) -> float:
    """Nearest-rank percentile (0-100) over a list of floats."""
    ordered = sorted(values)
    if not ordered:
        return 0.0
    k = max(0, min(len(ordered) - 1, int(round((pct / 100.0) * (len(ordered) - 1)))))
    return ordered[k]


@contextlib.contextmanager
def maybe_server(binary: str | None = None):
    """Yield an ApexServer, skipping the test if no binary is available."""
    if not (binary or find_binary()):
        import pytest

        pytest.skip("no ApexKit binary available (set APEXKIT_BIN or --bin)")
    server = ApexServer(binary=binary)
    try:
        server.start()
        yield server
    finally:
        server.stop()

