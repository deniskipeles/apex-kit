# Authentication and Security Policies

ApexKit provides a robust, built-in authentication system and a powerful policy engine to secure your data.

## Authentication Methods

### Email/Password
The most common way to authenticate users.
```javascript
const authData = await apex.auth.login('user@example.com', 'password123');
```

### Social Auth (OAuth2)
ApexKit supports GitHub and Google out of the box. Configure your Client ID and Secret in the **Settings** tab.
```javascript
// This will redirect the user to GitHub
apex.auth.loginWithGithub('http://your-app.com/callback');
```

## Security Policies (Rules)

Policies define who can access what. Every collection has four hooks: `read`, `create`, `update`, and `delete`.

### Policy Types

1. **Public (`public`)**: Anyone can perform the action.
2. **Authenticated (`auth`)**: Any logged-in user can perform the action.
3. **Admin (`admin`)**: Only users with the `admin` role can perform the action.
4. **Owner (`owner:field`)**: Only the user whose ID matches the value in the specified field can perform the action.
   - Example: `owner:author_id` ensures only the author of a post can edit it.
5. **Expression**: Advanced logic using a DSL.
   - Example: `auth.id == record.user_id || auth.role == 'moderator'`

### Configuring Policies
In the Dashboard, select a collection and go to **Access Control**. You can set different rules for each operation.

## JWT and Scoping

When a user logs in, they receive a JWT. This token contains their ID, role, and **Scope**.

### Scope Types
- **Root**: Global access across the entire instance.
- **Tenant**: Access limited to a specific tenant's data.
- **Sandbox**: Access limited to a specific sandbox session.

The SDK handles token management automatically once `setToken` or `login` is called.

## API Keys

For server-to-server communication or background tasks, you can generate **API Keys** in the dashboard.
- API Keys can be scoped to specific roles and tenants.
- They bypass traditional user login flows but are still subject to security policies.

## Best Practices
- **Always use `owner` policies** for user-generated content.
- **Set `delete` policies to `admin`** by default for critical data.
- **Use Scopes** to isolate customer data in B2B applications.
