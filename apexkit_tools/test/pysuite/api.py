"""Small helpers shared across the ApexKit Python test modules.

Keeps individual test files short and focused on assertions rather than HTTP
plumbing.
"""

from __future__ import annotations

from typing import Any

import requests

PUBLIC = {
    "read": "public",
    "create": "public",
    "update": "public",
    "delete": "public",
}

ADMIN_ONLY = {
    "read": "admin",
    "create": "admin",
    "update": "admin",
    "delete": "admin",
}


def ok(resp: requests.Response, *expected: int) -> Any:
    """Assert a response status and return its JSON body (or None)."""
    codes = expected or (200, 201)
    assert resp.status_code in codes, (
        f"unexpected status {resp.status_code} for {resp.request.method} "
        f"{resp.request.url}: {resp.text[:400]}"
    )
    if not resp.content:
        return None
    ctype = resp.headers.get("content-type", "")
    if "json" in ctype:
        return resp.json()
    return resp.text


def json_field(server, method: str, path: str, **kw: Any) -> Any:
    """Issue a request through the harness and return the parsed JSON body."""
    resp = server.request(method, path, **kw)
    return ok(resp)


def text_type(required: bool = False, **extra: Any) -> dict:
    d = {"type": "text", "required": required}
    d.update(extra)
    return d


def number_type(required: bool = False, **extra: Any) -> dict:
    d = {"type": "number", "required": required}
    d.update(extra)
    return d


def bool_type(required: bool = False, **extra: Any) -> dict:
    d = {"type": "boolean", "required": required}
    d.update(extra)
    return d


def assert_problem(body: Any, status: int) -> None:
    """Assert a ProblemDetail-shaped error body."""
    assert isinstance(body, dict), f"expected problem detail, got {body!r}"
    assert body.get("status") == status, body
    assert "error" in body and "message" in body, body


def items_of(body: Any) -> list:
    """Normalize a listing response that may be a bare list or {items: [...]}."""
    if isinstance(body, list):
        return body
    if isinstance(body, dict):
        for key in ("items", "data", "results"):
            if isinstance(body.get(key), list):
                return body[key]
    return []

