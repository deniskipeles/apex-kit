"""Authentication routes: login, registration policy, roles, /me."""

from __future__ import annotations

import uuid

import api


def test_admin_login_returns_token(server):
    r = server.post(
        "/api/v1/auth/login",
        json={"email": "admin@apexkit.io", "password": "password"},
    )
    body = api.ok(r)
    assert body["token"]
    assert body["user"]["role"] == "admin"


def test_login_wrong_password_is_401(server):
    r = server.post(
        "/api/v1/auth/login",
        json={"email": "admin@apexkit.io", "password": "wrong"},
    )
    assert r.status_code == 401


def test_login_unknown_user_is_401(server):
    r = server.post(
        "/api/v1/auth/login",
        json={"email": f"ghost-{uuid.uuid4().hex}@apexkit.io", "password": "x"},
    )
    assert r.status_code == 401


def test_register_disabled_by_default(server):
    server.enable_public_registration(False)
    r = server.register(f"nobody-{uuid.uuid4().hex}@apexkit.io", "secret123")
    assert r.status_code == 403
    api.assert_problem(r.json(), 403)
    server.enable_public_registration(True)


def test_register_when_enabled(server):
    server.enable_public_registration(True)
    email = f"user-{uuid.uuid4().hex}@apexkit.io"
    body = api.ok(server.register(email, "secret123"))
    assert body["token"]
    assert body["user"]["email"] == email
    assert body["user"]["role"] == "user"


def test_register_cannot_self_assign_admin(server):
    server.enable_public_registration(True)
    body = api.ok(server.register(f"wannabe-{uuid.uuid4().hex}@apexkit.io", "secret123", role="admin"))
    assert body["user"]["role"] == "user"


def test_register_duplicate_fails(server):
    server.enable_public_registration(True)
    email = f"dup-{uuid.uuid4().hex}@apexkit.io"
    api.ok(server.register(email, "secret123"))
    r = server.register(email, "secret123")
    assert not r.ok


def test_me_returns_current_user(server):
    body = api.ok(server.get("/api/v1/auth/me"))
    assert body["email"] == "admin@apexkit.io"
    assert body["role"] == "admin"


def test_me_requires_auth(server):
    r = server.get("/api/v1/auth/me", headers={"Authorization": ""})
    assert r.status_code == 401


def test_roles_endpoint_lists_roles(server):
    body = api.ok(server.get("/api/v1/auth/roles"))
    assert "roles" in body
    assert "admin" in body["roles"]


def test_invalid_token_is_unauthorized(server):
    r = server.get("/api/v1/auth/me", headers={"Authorization": "Bearer not.a.jwt"})
    assert r.status_code == 401
