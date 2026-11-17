"""Storage routes: upload, metadata, listing, serving, and deletion."""

from __future__ import annotations

import io

import api


def test_upload_and_fetch_file(server):
    r = server.post(
        "/api/v1/storage/upload",
        files={"file": ("note.txt", io.BytesIO(b"hello apexkit"), "text/plain")},
    )
    body = api.ok(r)
    assert body["id"]
    assert body["filename"].endswith(".txt")

    meta = api.ok(server.get(f"/api/v1/storage/files/{body['id']}"))
    assert meta["original_name"] == "note.txt"
    assert meta["size"] == len(b"hello apexkit")

    # Cleanup.
    server.delete(f"/api/v1/storage/files/{body['id']}")


def test_uploaded_file_is_served(server):
    body = api.ok(
        server.post(
            "/api/v1/storage/upload",
            files={"file": ("served.txt", io.BytesIO(b"contents"), "text/plain")},
        )
    )
    r = server.get(body["url"])
    assert r.status_code == 200
    assert r.content == b"contents"
    server.delete(f"/api/v1/storage/files/{body['id']}")


def test_list_files(server):
    body = api.ok(server.get("/api/v1/storage/files"))
    assert "items" in body
    assert "total" in body


def test_delete_file(server):
    body = api.ok(
        server.post(
            "/api/v1/storage/upload",
            files={"file": ("gone.txt", io.BytesIO(b"x"), "text/plain")},
        )
    )
    r = server.delete(f"/api/v1/storage/files/{body['id']}")
    assert r.status_code in (200, 204)
    assert server.get(f"/api/v1/storage/files/{body['id']}").status_code == 404


def test_upload_without_auth_is_rejected(server):
    r = server.post(
        "/api/v1/storage/upload",
        files={"file": ("x.txt", io.BytesIO(b"x"), "text/plain")},
        headers={"Authorization": ""},
    )
    assert r.status_code == 401


def test_get_missing_file_is_404(server):
    r = server.get("/api/v1/storage/files/999999")
    assert r.status_code == 404
