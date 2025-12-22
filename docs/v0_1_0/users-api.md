# ApexKit Users API & Data Relationships

**Version:** 2.1
**Base URL:** `http://localhost:5000/api/v1`

In ApexKit, **Users** are a system-level entity distinct from standard Data Collections. The Users API handles authentication, identity management, and security roles.

This document explains how to manage users and, crucially, **how to link Users to Data Records** to create personalized, secure applications.

---

## 1. The User Object

The system User object is rigid and optimized for authentication. It is **not** a schema-less JSON document like a standard Record.

```json
{
  "id": 101,
  "email": "developer@example.com",
  "role": "user",  // "admin" or "user"
  "last_active": "2023-10-27T10:00:00Z"
}
```

*   **id**: (Integer) Unique System ID.
*   **role**: Defines high-level system access.
    *   `admin`: Can modify Schemas, Settings, and delete any data.
    *   `user`: Subject to Collection Policies (e.g., can only edit their own data).

---

## 2. Authentication Endpoints

These endpoints are public and used to establish a session.

### Register
Create a new account.
*   **POST** `/auth/register`

**Body:**
```json
{
  "email": "alice@example.com",
  "password": "securePassword123"
}
```
**Response:** Returns the User object + JWT Token.

### Login
Authenticate and receive a session token.
*   **POST** `/auth/login`

**Body:** Same as Register.

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1Ni...",
  "user": { ... }
}
```

> **Client Usage:** Store the `token` in LocalStorage/Cookies and send it in the header `Authorization: Bearer <token>` for all subsequent requests.

---

## 3. Relating Users to Data Records

This is the most critical concept for application development. Since the `users` table is fixed, how do you store a profile picture? How do you ensure a user only sees their own posts?

### A. The "Owner" Field Type
When designing a Collection Schema, you can define a special field type called `owner`.

**Collection Schema Example (`posts`):**
```json
{
  "fields": {
    "title": { "type": "string", "required": true },
    "author_id": { "type": "owner", "required": true }
  }
}
```

When you create a record in this collection, the `author_id` field expects a User ID.

### B. Row-Level Security (Ownership Policy)
You enforce data ownership using **API Policies** in your collection definition.

**The Policy:** `"update": "owner:author_id"`

**How it works:**
1.  Alice (User ID `10`) tries to `PATCH /collections/posts/records/55`.
2.  Record `55` has `{"author_id": 10, "title": "Alice's Post"}`.
3.  ApexKit compares the Token ID (`10`) with the Record's `author_id` (`10`).
4.  **Match:** Update allowed.

If Bob (User ID `20`) tries to update Record `55`, the IDs won't match, and he receives `403 Forbidden`.

### C. Storing User Profiles (The "Sidecar" Pattern)
Because the system `User` object cannot hold extra fields like `avatar` or `bio`, you should create a standard Collection named `profiles`.

1.  **Create Collection `profiles`**:
    *   Fields:
        *   `user_id`: `{ "type": "owner", "unique": true, "required": true }`
        *   `full_name`: `{ "type": "string" }`
        *   `avatar`: `{ "type": "file" }`
    *   Policies:
        *   `create`: `"auth"`
        *   `update`: `"owner:user_id"`
        *   `read`: `"public"` (If profiles are public)

2.  **Usage**:
    When a user registers, your frontend (or a server-side script trigger) creates a record in `profiles` linking to the new User ID.

---

## 4. Admin Management

Endpoints for administrators to manage the user base. Requires `Authorization` header with an admin token.

### List Users
**GET** `/admin/users`

Returns a list of all registered users.

### Delete User
**DELETE** `/admin/users/{id}`

*   Deletes the user account.
*   **Note on Cascading:** Currently, this does *not* automatically delete records in other collections linked via `owner` fields (to prevent accidental data loss). You should use a `before_delete` script or manual cleanup if that is required.

---

## 5. Advanced Auth (OAuth & Verification)

### GitHub Login
*   **GET** `/auth/github` -> Redirects browser to GitHub.
*   **GET** `/auth/github/callback` -> Returns JWT + User after successful GitHub auth.

*(Requires `github_client_id` and `github_client_secret` to be set in System Config)*

### Email Verification
*   **POST** `/auth/verify/resend`: `{ "email": "..." }` -> Sends email.
*   **GET** `/auth/verify?token=...`: Verifies the account.

---

## 6. Example: Fetching "My Data"

To fetch records belonging to the currently logged-in user, you typically filter by the owner field.

**Scenario:** Get all orders for the current user.

1.  **Frontend:** Get current User ID from the stored session (e.g., `user.id = 5`).
2.  **Request:**
    `GET /collections/orders/records?filter={"user_id": 5}`

**Secure it:**
Even though the filter is sent by the client, you ensure security by setting the collection's **Read Policy** to `"owner:user_id"`.
If a user tries to change the filter to `?filter={"user_id": 6}`, the API Policy check will fail because the returned records do not belong to the token bearer, returning an empty list or `403`.