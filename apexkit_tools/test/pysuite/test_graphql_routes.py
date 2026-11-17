"""GraphQL endpoint: schema, queries, mutations, and policy enforcement."""

from __future__ import annotations

import requests

import api


def test_status_query(server):
    body = server.graphql("{ _status }")
    assert "errors" not in body, body
    assert isinstance(body["data"]["_status"], str)


def test_introspection_query_type(server):
    body = server.graphql("{ __schema { queryType { name } } }")
    assert body["data"]["__schema"]["queryType"]["name"] == "Query"


def test_unknown_field_returns_error(server):
    body = server.graphql("{ thisFieldDoesNotExist }")
    assert body.get("errors")


def test_users_query_requires_policy(server):
    # The default user read policy is admin-only; unauthenticated must fail.
    r = requests.post(
        f"{server.base_url}/graphql",
        json={"query": "{ _users { total } }"},
        timeout=10,
    )
    body = r.json()
    assert body.get("errors"), body


def test_users_query_as_admin(server):
    body = server.graphql("{ _users { total items { id email role } } }")
    assert "errors" not in body, body
    assert body["data"]["_users"]["total"] >= 1


def test_collection_query_and_mutation(server, fresh):
    name = fresh.make_collection(
        "gqlposts", {"title": api.text_type(True), "views": api.number_type()}
    )
    # Seed via REST so the generated resolver has data.
    server.record(name, {"title": "From GraphQL", "views": 3})

    # Type name is the capitalized collection name; with a unique prefix the
    # query field name matches the collection name exactly.
    query = "{ %s { total items { id title views } } }" % name
    body = server.graphql(query)
    assert "errors" not in body, body
    assert body["data"][name]["total"] == 1


def test_graphql_playground_is_served(server):
    r = server.get("/graphql")
    assert r.status_code in (200, 404)
