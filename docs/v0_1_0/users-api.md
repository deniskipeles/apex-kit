# 👤 Users API & Identity Management

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

In ApexKit, **Users** are system-level entities managed within an isolated `core.db` for every tenant. Unlike standard Data Collections, the User schema is rigid and optimized for authentication, though it supports a flexible `metadata` object for lightweight extensions.

---

## 1. The User Object

The User object represents a unique identity within a specific scope (Root, Tenant, or Sandbox).

```json
{
  "id": 101,
  "email": "user@example.com",
  "role": "admin",
  "scope": "tenant:client-abc",
  "is_verified": true,
  "metadata": {
    "avatar": "profile_123.jpg",
    "theme_preference": "dark"
  }
}
```

*   **id**: (Integer) Unique System ID.
*   **role**: High-level permission tier (e.g., `admin`, `user`).
*   **scope**: The environment the user belongs to. Tokens are strictly valid only for this scope.
*   **metadata**: A JSON object for storing non-structural user data.

---

## 2. Authentication Endpoints

### Login
Authenticate and receive a JWT token.
*   **POST** `/auth/login`
*   **Body**: `{ "email": "...", "password": "..." }`
*   **Response**: `{ "token": "...", "user": { ... } }`

### Register
Create a new account. Registration can be disabled via Root Settings.
*   **POST** `/auth/register`
*   **Body**: `{ "email": "...", "password": "...", "metadata": { ... } }`

### Identity (Me)
Retrieve the profile of the currently logged-in user.
*   **GET** `/auth/me`
*   **Auth**: Required (Bearer Token)

---

## 3. Scopes & Permission Levels

ApexKit enforces a strict hierarchy to prevent cross-tenant data leakage.

| Role | Scope | Capabilities |
| :--- | :--- | :--- |
| **Root Admin** | `root` | Full access to the master system and **all** tenants/sandboxes. |
| **Tenant Admin** | `tenant:{id}` | Full access to their specific tenant's settings and data. |
| **Standard User** | Any | Access to data based on **Collection Policies** (RLS). |

---

## 4. Relating Users to Data (RLS)

The primary way to personalize applications is by linking Data Records to Users using the `owner` field type.

### The `owner` Field
When a field is defined as type `owner` in a collection schema, it stores the User ID.

**Schema Example:**
```json
"author_id": { 
  "type": "owner", 
  "auto": true,   // Automatically injects current User ID on Create
  "required": true 
}
```

### Row-Level Security (RLS)
You enforce ownership using **Collection Policies**.

*   **Policy**: `auth.id == field:author_id`
*   **Logic**: If User #10 tries to update a record where `author_id` is #10, access is granted. If the IDs mismatch, the API returns `403 Forbidden`.

---

## 5. User Profiles (The Sidecar Pattern)

Because the system `User` object is not customizable, use a **Sidecar Collection** for complex profile data (e.g., bios, addresses, social links).

1.  **Create Collection `profiles`**:
    *   `user_id`: `{ "type": "owner", "unique": true, "auto": true }`
    *   `bio`: `{ "type": "text" }`
    *   `location`: `{ "type": "string" }`
2.  **Accessing**: 
    In your frontend, fetch the profile using a filter:  
    `GET /collections/profiles/records?filter={"user_id": current_user_id}`

---

## 6. Auth Hooks (Edge Functions)

You can intercept user lifecycle events using scripts to add custom validation or logic.

*   **`before_user_create`**: Validate email domains or block specific registrations.
*   **`after_user_create`**: Automatically create a record in the `profiles` sidecar collection.
*   **`before_user_delete`**: Clean up associated data or block deletion of "Super Admins".

---

## 7. JavaScript SDK Usage

The `ApexKit` client manages token persistence and scope automatically.

```javascript
import { pb } from './apiClient';

// 1. Authenticate
const { token, user } = await pb.auth.login('alice@app.com', 'password');

// 2. Fetch current user (validates token/scope)
const me = await pb.auth.getMe();
console.log(`Loggend into: ${me.scope}`);

// 3. Admin Management (Requires Admin role)
const { items: users } = await pb.admins.listUsers({ page: 1 });

// 4. Update Metadata
await pb.admins.updateUser(user.id, {
    metadata: { ...user.metadata, last_ip: '1.2.3.4' }
});
```

---

## 8. Advanced: API Keys

For server-to-server communication, use **API Keys** instead of user credentials.
*   **Root API Keys**: Can have scope `*` (Global) or `tenant:{id}`.
*   **Bypass CORS**: Keys can be configured to ignore browser Origin checks, useful for native mobile integrations.
*   **Header**: `x-api-key: ak_...`