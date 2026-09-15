# Authentication

The `apex.auth` namespace provides methods for user management, identity resolution, social OAuth2 logins, and authorization claims.

## Methods

- `auth.login(email, password)`
- `auth.register(email, password, metadata?, role?)`
- `auth.getMe()`
- `auth.updateMeMetadata(metadata?)`
- `auth.logout()`
- `auth.loginWithGithub(redirectTo?)`
- `auth.loginWithGoogle(redirectTo?)`
- `auth.requestPasswordReset(email)`
- `auth.confirmPasswordReset(token, newPassword)`
- `auth.listRoles()`

### Login / Register

```typescript
const authRes = await apex.auth.login('user@email.com', 'password');
console.log(authRes.user.id, authRes.token);

// Registration
const regRes = await apex.auth.register('new-user@email.com', 'password', { displayName: 'Alice' }, 'user');
```

The `AuthResponse` includes:
- `token`: The JWT authorization string.
- `user`: The `User` object (`id`, `email`, `role`, `scope`, `metadata`).

The SDK automatically stores and attaches this token to subsequent outgoing requests.

### Identity & Profile Management

Get the profile of the currently logged-in user or update their metadata.

```typescript
// Fetch current profile
const me = await apex.auth.getMe();
console.log(`Current Scope: ${me.scope}`); // e.g., "tenant:client-alpha"

// Update current user metadata
const updatedUser = await apex.auth.updateMeMetadata({
    bio: "Full-stack engineer",
    theme: "dark"
});
```

### Logout

Clears the token and user data from the SDK client instance.

```typescript
apex.auth.logout();
```

### OAuth (GitHub/Google)

Triggers a browser redirect to the respective OAuth provider.

```typescript
// GitHub OAuth
apex.auth.loginWithGithub('https://your-frontend.com/callback');

// Google OAuth
apex.auth.loginWithGoogle('https://your-frontend.com/callback');
```

### Password Resets

```typescript
// Request password reset email
await apex.auth.requestPasswordReset('user@email.com');

// Confirm and update password using the token received in email
await apex.auth.confirmPasswordReset('token_from_email_link', 'newSecurePassword123');
```

## Types Reference

For detailed definitions of `User`, `AuthResponse`, and other types, see [Types Reference](types.md).
