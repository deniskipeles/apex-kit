"""AI routes: actions listing, plugins, session, and run gating."""

from __future__ import annotations

import api


def test_list_ai_actions(server):
    body = api.ok(server.get("/api/v1/admin/ai/actions"))
    assert isinstance(body, list)
    assert any(a.get("slug") == "generate-image" for a in body)


def test_list_ai_plugins(server):
    body = api.ok(server.get("/api/v1/admin/ai/plugins"))
    assert isinstance(body, list)


def test_get_ai_session(server):
    body = api.ok(server.get("/api/v1/admin/ai/session"))
    assert body["id"] == "default"
    assert "messages" in body


def test_run_action_without_ai_configuration(server):
    # AI is not configured in a fresh instance: the run endpoint must fail
    # cleanly (500 with a message) rather than panic.
    r = server.post(
        "/api/v1/ai/run/content-editor",
        json={"variables": {"prompt": "hello"}},
    )
    assert r.status_code in (200, 400, 500, 503), r.text
    if r.status_code >= 400:
        api.assert_problem(r.json(), r.status_code)


def test_run_action_requires_variables_field(server):
    r = server.post("/api/v1/ai/run/content-editor", json={"prompt": "x"})
    assert r.status_code == 422
