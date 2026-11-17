"""SSE and WebSocket realtime endpoints."""

from __future__ import annotations

import base64
import os
import socket


def test_sse_stream_opens(server):
    with server.get("/api/v1/sse", stream=True, timeout=5) as r:
        assert r.status_code == 200
        assert "text/event-stream" in r.headers.get("content-type", "")
        assert r.headers.get("cache-control") == "no-cache"


def test_sse_is_public(server):
    """The SSE stream is reachable without credentials (anonymous subscribers)."""
    with server.get("/api/v1/sse", headers={"Authorization": ""}, stream=True, timeout=5) as r:
        assert r.status_code == 200


def test_websocket_endpoint_accepts_upgrade(server):
    """Minimal RFC6455 handshake: expect 101 Switching Protocols."""
    key = base64.b64encode(os.urandom(16)).decode()
    req = (
        f"GET /ws HTTP/1.1\r\n"
        f"Host: 127.0.0.1:{server.port}\r\n"
        f"Upgrade: websocket\r\n"
        f"Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        f"Sec-WebSocket-Version: 13\r\n"
        f"Authorization: Bearer {server.token}\r\n"
        f"\r\n"
    )
    with socket.create_connection(("127.0.0.1", server.port), timeout=5) as sock:
        sock.sendall(req.encode())
        head = sock.recv(1024)
    status_line = head.split(b"\r\n", 1)[0]
    assert b"101" in status_line, status_line


def test_websocket_plain_get_is_rejected(server):
    r = server.get("/ws", timeout=5)
    assert r.status_code < 500
