# Authentication

The `apex.auth` namespace provides methods for user management, login, and authorization.

## Methods

- `auth.login(email, password)`
- `auth.register(email, password)`
- `auth.getMe()`
- `auth.logout()`
- `auth.loginWithGithub(redirectTo?)`
- `auth.loginWithGoogle(redirectTo?)`
- `auth.listRoles()`

### Login / Register

```typescript
const authRes = await apex.auth.login('user@email.com', 'password');
console.log(authRes.user.id, authRes.token);

// Registration
const regRes = await apex.auth.register('new-user@email.com', 'password');
```

The `AuthResponse` includes:
- `token`: The JWT string.
- `user`: The `User` object.

The SDK automatically sets the token for subsequent requests.

### Identity

Get the profile of the currently logged-in user.

```typescript
const me = await apex.auth.getMe();
console.log(`Current Scope: ${me.scope}`); // e.g., "tenant:client-alpha"
```

### Logout

Clears the token and user data from the SDK instance.

```typescript
apex.auth.logout();
```

### OAuth (GitHub/Google)

Triggers a redirect to the respective OAuth provider.

```typescript
// GitHub OAuth (Browser environment)
apex.auth.loginWithGithub('https://your-frontend.com/callback');

// Google OAuth (Browser environment)
apex.auth.loginWithGoogle('https://your-frontend.com/callback');
```

## Types Reference

For detailed definitions of `User`, `AuthResponse`, and other types, see [Types Reference](types.md).
