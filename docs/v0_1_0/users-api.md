# 👤 Users API & Identity Management

**Version:** 0.1.0  
**Base URL:** `https://api.your-app.com/api/v1`

In ApexKit, **Users** are system-level identity profiles managed within an isolated `core.db` partition for each tenant or root environment. Unlike standard Data Collections, the User schema has a fixed structure optimized for authentication and authorization, while supporting a flexible `metadata` JSON object for profile extensions.

---

## 1. The User Object

The User model represents a unique identity bound to a specific scope (Root, Tenant, or Sandbox).

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

* **`id`**: Unique integer primary key (`INTEGER`).
* **`email`**: Unique user email address used for credential lookups.
* **`role`**: High-level permission tier (e.g., `admin`, `user`, or custom roles).
* **`scope`**: The environment boundary the user belongs to (e.g. `root`, `tenant:client-abc`). Tokens are strictly valid only within their matching scope.
* **`is_verified`**: Boolean flag indicating whether the email address has been verified.
* **`metadata`**: Structured JSON object for storing arbitrary user metadata.

---

## 2. Authentication & Management Endpoints

### A. Core Auth Endpoints
* **Login:** `POST /auth/login` (Body: `{ "email": "...", "password": "..." }` ➔ Returns `{ "token": "...", "user": { ... } }`)
* **Register:** `POST /auth/register` (Body: `{ "email": "...", "password": "...", "metadata": { ... } }`)
* **Current Profile:** `GET /auth/me` (Requires Bearer Token)
* **Update Current Profile:** `PATCH /auth/me` (Body: `{ "metadata": { ... } }`)
* **Available Roles:** `GET /auth/roles` (Returns `{ "roles": ["admin", "user", ...] }`)
* **Email Verification:** `GET /auth/verify?token=...`
* **Resend Verification:** `POST /auth/verify/resend` (Body: `{ "email": "..." }`)
* **Password Reset Request:** `POST /auth/request-password-reset` (Body: `{ "email": "..." }`)
* **Password Reset Confirmation:** `POST /auth/confirm-password-reset` (Body: `{ "token": "...", "new_password": "..." }`)

---

### B. Admin User Management Endpoints
All admin endpoints require `role: "admin"` and a valid token or API key.

* **List Users:** `GET /admin/users?page=1&per_page=20&search=...`
* **Update User:** `PATCH /admin/users/{id}` (Body: `{ "email": "...", "role": "...", "password": "...", "metadata": { ... } }`)
* **Delete User:** `DELETE /admin/users/{id}`

---

## 3. Scopes & Permission Levels

ApexKit enforces a strict scope hierarchy to prevent cross-tenant data leakage:

| Role | Scope Context | Capabilities |
| :--- | :--- | :--- |
| **Root Admin** | `root` | Global system control over infrastructure, metrics, configurations, and all tenant workspaces. |
| **Tenant Admin** | `tenant:{id}` | Full administrative control over users, collections, and settings within their specific tenant. |
| **Standard User** | Any | Access restricted by collection-level security policies (RLS). |

---

## 4. Relating Users to Data (Row-Level Security)

The primary mechanism for restricting data access to specific users is the `owner` field type.

### The `owner` Field Definition
When a field is defined as type `owner` in a collection schema, it stores the user's numeric ID (`auth.id`). Setting `"auto": true` ensures the system automatically populates the field with the currently authenticated caller's ID during record creation.

```json
"author_id": {
  "type": "owner",
  "auto": true,
  "required": true
}
```

### Enforcing Row-Level Security (RLS)
Security policies evaluate user ownership at the database level:
* **Policy Rule:** `admin || owner:author_id`
* **Logic:** If User #10 tries to update a record where `author_id` is #10, access is granted. If User #11 attempts the same action, the API returns `403 Forbidden`.

---

## 5. User Profiles (The Sidecar Pattern)

Because the core `users` table has a fixed schema optimized for authentication, use a **Sidecar Collection** for extended profile data (such as bios, addresses, or avatars):

1. **Create Collection `profiles`**:
   * `user_id`: `{ "type": "owner", "unique": true, "auto": true }`
   * `bio`: `{ "type": "text" }`
   * `location`: `{ "type": "string" }`
2. **Accessing Profiles via REST/SDK**:
   Query the sidecar collection using the user ID filter:
   ```http
   GET /api/v1/collections/profiles/records?filter={"user_id": 101}
   ```

---

## 6. Auth Hooks (Edge Functions)

You can intercept user lifecycle events using scripts registered to system triggers:

* **`before_user_create`**: Validate email domains or block unauthorized registrations.
* **`after_user_create`**: Automatically create a linked record in the `profiles` sidecar collection or dispatch a welcome email.
* **`before_user_delete`**: Clean up associated user files or block deletion of critical accounts.

---

## 7. TypeScript SDK Usage (`@apexkit/sdk`)

```typescript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-app.com');

// 1. Authenticate user
const { token, user } = await apex.auth.login('alice@example.com', 'password123');
console.log(`Logged in as User ID: ${user.id} in scope: ${user.scope}`);

// 2. Fetch fresh user profile
const me = await apex.auth.getMe();

// 3. Update current user metadata
await apex.auth.updateMeMetadata({
  theme: 'dark',
  avatar: 'avatar_99.png'
});

// 4. Admin Operations (Requires Admin Role)
const allUsers = await apex.admins.listUsers({ page: 1, per_page: 20 });
console.log(`Total users: ${allUsers.total}`);

// Create a new user as an admin
const createdUser = await apex.admins.registerUser(
  'bob@example.com',
  'strongPass123!',
  'user',
  { department: 'engineering' }
);

// Update user role or email
await apex.admins.updateUser(createdUser.user.id, {
  role: 'editor'
});

// Delete user
await apex.admins.deleteUser(createdUser.user.id);
```