# Tinybase Development Guide

This document provides a comprehensive guide for developers contributing to the Tinybase application.

## 1. Application Overview

### Purpose
Tinybase is a robust, scalable, and production-grade RESTful API designed to be a flexible backend solution. The initial implementation provides a foundation for creating and managing data through a versioned API.

### Architecture
The project is structured as a Rust workspace to promote modularity and clean separation of concerns. It consists of two main crates:

-   `tinybase-core`: This crate contains the core business logic, database interaction patterns, and data models. A key feature is the generic `Db` trait, which abstracts database connections. This allows the application to seamlessly switch between a production-ready connection pool and a single, in-memory connection for isolated testing.
-   `tinybase-api`: This is the main web server, built with the Axum framework. It's responsible for defining API routes, handling incoming requests, and managing error responses.

### Technology Stack
-   **Rust:** The primary programming language, chosen for its performance, safety, and modern concurrency features.
-   **Axum:** A highly ergonomic and modular web framework built by the Tokio team.
-   **Tokio:** The asynchronous runtime that powers the application.
-   **libSQL:** A fast, local-first database engine (an open-source fork of SQLite).
-   **Serde:** The standard for efficient and reliable JSON serialization and deserialization in Rust.

## 2. Setup Process

### Prerequisites
-   A working Rust development environment. You can install it using `rustup` from [rust-lang.org](https://www.rust-lang.org/tools/install).

### Steps
1.  **Clone the Repository:**
    ```bash
    git clone <repository-url>
    cd tinybase
    ```
2.  **Build the Project:**
    Compile the entire workspace to ensure all dependencies are fetched and the code is valid.
    ```bash
    cargo build
    ```
3.  **Run the Tests:**
    Verify that your environment is set up correctly by running the integration test suite.
    ```bash
    cargo test
    ```
    All tests should pass.

4.  **Run the Application:**
    Start the API server.
    ```bash
    cargo run --bin tinybase-api
    ```
    The server will be available at `http://0.0.0.0:3000`.

## 3. Issues Encountered & Solutions

This section documents the key challenges faced during the initial development and the solutions that were implemented.

### 1. Test Isolation with In-Memory libSQL
-   **Problem:** The integration tests were failing with `"SQLite failure: no such table: collections"`. The root cause was that each connection to a `:memory:` libSQL database creates a new, completely isolated database. The test setup was creating the schema on one connection, but the Axum handlers were acquiring a different connection from the pool, which pointed to a fresh, empty database.
-   **Solution:** A generic `Db` trait was introduced in `tinybase-core` to abstract all database operations. Two implementations were provided:
    -   For production (`libsql::Database`): Uses a connection pool for performance.
    -   For tests (`Arc<Mutex<libsql::Connection>>`): Uses a single, in-memory connection wrapped in a mutex. This ensures that the schema is created and accessed on the *same* connection, providing complete test isolation and fixing the "no such table" error.

### 2. Dependency Version Mismatches
-   **Problem:** During the refactoring of the tests to use `tower::ServiceExt::oneshot`, compilation errors occurred due to incompatible versions of the `hyper` and `tower` crates with the version of `axum` being used.
-   **Solution:** The versions of `hyper` and `tower` in the `dev-dependencies` were updated to align with the versions used by `axum`, resolving the trait bound conflicts.

### 3. Evolution of Error Handling
-   **Problem:** The initial implementation used `.unwrap()` extensively, which would cause the server to panic on any error. This is not suitable for a production-grade application.
-   **Solution:** A robust error handling system was implemented:
    -   A custom `AppError` enum was created to represent different error types (e.g., database errors, serialization errors).
    -   This enum implements Axum's `IntoResponse` trait to convert internal errors into structured JSON responses that conform to RFC 7807 (`ProblemDetail`), providing clear and consistent error information to API clients.
    -   All `.unwrap()` calls were replaced with proper error handling.

### 4. Correct HTTP Semantics
-   **Problem:** The `POST` endpoints for creating resources were initially returning a `200 OK` status code.
-   **Solution:** The handlers were updated to return a `201 Created` status code, which is the correct HTTP semantic for successful resource creation. The integration tests were also updated to assert for this status code.

## 4. Plan Forward

The current implementation provides a solid foundation, but there are many opportunities for expansion. Future contributors can refer to the original `README.md` for the full vision. The following is a suggested roadmap for the next development phases:

### Phase 1: Core API Features
1.  **Input Validation & Sanitization:**
    -   Implement a strong validation layer for all incoming requests using a schema engine like `serde_valid` or `validator`.
    -   Enforce required fields, type checking, string length limits, and other constraints.
    -   Sanitize inputs to prevent common security vulnerabilities.
2.  **Authentication & Authorization:**
    -   Implement JWT-based authentication with short-lived access tokens and refresh tokens.
    -   Establish a Role-Based Access Control (RBAC) system to enforce permissions on a per-endpoint basis.
3.  **Pagination & Filtering:**
    -   Implement cursor-based pagination for all list endpoints to ensure scalability.
    -   Design a robust filtering system that allows clients to query records based on specific criteria.

### Phase 2: Performance & Scalability
1.  **Caching:**
    -   Introduce a caching layer (e.g., in-memory LRU or Redis) for frequently accessed data, such as collection schemas and heavily used records.
2.  **Background Tasks:**
    -   Move slow or long-running operations (e.g., sending emails, processing images) out of the request-response cycle and into a background job queue.

### Phase 3: Monitoring & Production Readiness
1.  **Logging & Monitoring:**
    -   Integrate the `tracing` crate to provide detailed, structured logs with request IDs.
    -   Expose a `/metrics` endpoint for Prometheus to scrape key application metrics.
2.  **Rate Limiting:**
    -   Implement a rate-limiting middleware to protect the API from abuse.

This roadmap is a guide, and the priority of these features can be adjusted based on the project's evolving needs.
