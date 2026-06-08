export const AuthDocs = {
  registrationAndLogin: `// 1. Register a new user
const res = await client.auth.register('user@example.com', 'password123');

// 2. Login (Token is automatically cached in the SDK instance)
const authData = await client.auth.login('user@example.com', 'password123');

console.log("JWT Token:", authData.token);
console.log("User Data:", authData.user);

// 3. Fetch current logged-in profile (Requires valid token)
const me = await client.auth.getMe();

// 4. Logout (Clears internal token)
client.auth.logout();`,

  passwordResetRequest: `// Triggers an email to the user with a token
await fetch(client.baseUrl + '/api/v1/auth/request-password-reset', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ email: 'user@example.com' })
});`,

  passwordResetConfirm: `// User clicked the link in their email and is now on your site
// e.g., https://yourfrontend.com/reset-password?token=abc-123

const urlParams = new URLSearchParams(window.location.search);
const token = urlParams.get('token');

await fetch(client.baseUrl + '/api/v1/auth/confirm-password-reset', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ 
    token: token, 
    new_password: 'new_secure_password' 
  })
});`,

  emailVerificationResend: `// Triggers the verification email again
await fetch(client.baseUrl + '/api/v1/auth/verify/resend', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ email: 'user@example.com' })
});`,

  emailVerificationConfirm: `// The email contains a link like: 
// https://your-app-url.com/api/v1/auth/verify?token=abc-123
// This is a GET request. When clicked, it automatically verifies the user in the database.

// You can also hit it via code:
await fetch(client.baseUrl + '/api/v1/auth/verify?token=abc-123');`,

  oauthSetup: `// Redirects window.location to the OAuth consent screen.
// Once complete, ApexKit will redirect back to your specified callback URL.
// The resulting URL will have ?token=<jwt> appended to it.

client.auth.loginWithGoogle('https://myapp.com/auth-callback');

client.auth.loginWithGithub('https://myapp.com/auth-callback');

// --- On your frontend callback page ---
// const params = new URLSearchParams(window.location.search);
// const token = params.get('token');
// client.setToken(token);`,
};
