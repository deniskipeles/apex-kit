"""Isolated replication router: master-key protection and snapshot access."""

from __future__ import annotations


def test_replication_snapshot_requires_master_key(server):
    r = server.get("/replication/snapshot", params={"db_path": "storage/root.db"})
    assert r.status_code == 401


def test_replication_snapshot_rejects_path_traversal(server):
    r = server.get(
        "/replication/snapshot",
        params={"db_path": "storage/../../etc/passwd"},
        headers=server.master_headers(),
    )
    assert r.status_code == 403


def test_replication_snapshot_missing_file_is_404(server):
    # NOTE: the handler opens the SQLite path for a WAL checkpoint before
    # streaming, which creates the file when its parent directory exists.
    # A path whose directory does not exist therefore reaches the stream
    # step and yields 404.
    r = server.get(
        "/replication/snapshot",
        params={"db_path": "storage/no-such-dir/missing.db"},
        headers=server.master_headers(),
    )
    assert r.status_code == 404


def test_replication_snapshot_rejects_logs_db(server):
    r = server.get(
        "/replication/snapshot",
        params={"db_path": "storage/logs.db"},
        headers=server.master_headers(),
    )
    assert r.status_code == 403


def test_replication_snapshot_missing_param_is_400(server):
    r = server.get("/replication/snapshot", headers=server.master_headers())
    assert r.status_code == 400
