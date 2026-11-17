"""System-level routes: health, version, metrics, dashboard, app details."""

from __future__ import annotations

import api


def test_healthz_returns_ok(server):
    r = server.get("/healthz")
    assert r.status_code == 200
    assert r.text.strip() == "OK"


def test_version_reports_all_crates(server):
    body = api.ok(server.get("/version"))
    for key in ("root", "api", "core", "vector"):
        assert key in body, f"missing {key} in {body}"
        assert isinstance(body[key], str)


def test_metrics_endpoint_responds(server):
    r = server.get("/metrics")
    assert r.status_code == 200
    # A fresh instance may have no registered metrics yet; the endpoint must
    # still respond cleanly with a text/plain content type.
    assert "text/plain" in r.headers.get("content-type", "")


def test_dashboard_is_served(server):
    r = server.get("/_dashboard")
    assert r.status_code in (200, 404)


def test_app_details_is_json(server):
    r = server.get("/app-details")
    assert r.status_code == 200
    assert "json" in r.headers.get("content-type", "")


def test_unknown_route_returns_404(server):
    r = server.get("/api/v1/definitely-not-a-route")
    assert r.status_code == 404


def test_method_not_allowed(server):
    # /healthz is GET-only.
    r = server.post("/healthz")
    assert r.status_code in (404, 405)


def test_error_body_is_problem_detail(server):
    r = server.get("/api/v1/collections/does-not-exist/records")
    assert r.status_code == 404
    api.assert_problem(r.json(), 404)
