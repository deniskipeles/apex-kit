If you are building a custom frontend on Vercel (like a Next.js, React, or Vue app) and using ApexKit strictly as a headless backend, you **do not** want your users to be redirected to the ApexKit Admin UI for password resets. 

Instead, you want them to land on a page inside your Vercel application. 

Here is exactly how you handle that flow using the new `{{token}}` variable we just added:

### Step 1: Update your Email Templates in ApexKit
Go to your ApexKit Admin UI **Settings > Email (SMTP)**. Instead of using the default `{{link}}` variable, you will hardcode your Vercel app's URL and append the raw `{{token}}`.

**Change your Password Reset Template to:**
```text
Hello {{email}},

We received a request to reset your password for {{app_name}}. 
Please click the link below to choose a new password:

https://my-custom-app.vercel.app/reset-password?token={{token}}

If you did not request this, please ignore this email.
```

*(Do the same for your Email Verification template, pointing it to something like `https://my-custom-app.vercel.app/verify?token={{token}}`)*

---

### Step 2: The User Flow

1. **User forgets password:** The user goes to your Vercel app (`my-custom-app.vercel.app/login`) and clicks "Forgot Password".
2. **Request Reset:** Your Vercel app calls the ApexKit SDK:
   ```javascript
   await client.auth.requestPasswordReset("user@example.com");
   ```
3. **Email Sent:** ApexKit generates a secure UUID token (`abc-123-xyz`) and sends the email using your template.
4. **User clicks link:** The user receives the email and clicks:
   `https://my-custom-app.vercel.app/reset-password?token=abc-123-xyz`
5. **User lands on Vercel:** Your Vercel app reads the `token` from the URL and displays a form asking for a "New Password".

---

### Step 3: Handling the Reset on Vercel (React/Next.js Example)

Here is exactly what the code on your Vercel app's `/reset-password` page would look like using the ApexKit JS SDK:

```tsx
import { useState, useEffect } from 'react';
import { ApexKit } from '@apexkit/sdk';

const client = new ApexKit('https://api.your-apexkit-domain.com');

export default function ResetPasswordPage() {
  const [token, setToken] = useState<string | null>(null);
  const [newPassword, setNewPassword] = useState('');
  const [status, setStatus] = useState('');

  // 1. Grab the token from the URL when the page loads
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    setToken(params.get('token'));
  }, []);

  // 2. Submit the new password to ApexKit
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token) return;

    try {
      await client.auth.confirmPasswordReset(token, newPassword);
      setStatus('Password reset successful! You can now log in.');
      
      // Optional: Redirect them to your login page
      // window.location.href = '/login';
      
    } catch (err) {
      setStatus(`Error: ${err.message || 'The token is invalid or expired.'}`);
    }
  };

  if (!token) return <div>Invalid or missing reset token.</div>;

  return (
    <form onSubmit={handleSubmit}>
      <h2>Choose a New Password</h2>
      
      <input 
        type="password" 
        placeholder="New Password (min 6 chars)" 
        value={newPassword}
        onChange={(e) => setNewPassword(e.target.value)}
        required
      />
      
      <button type="submit">Update Password</button>
      
      <p>{status}</p>
    </form>
  );
}
```

### Important: Don't forget CORS!
Because your Vercel app (`https://my-custom-app.vercel.app`) is on a different domain than your ApexKit backend (`https://api.your-apexkit-domain.com`), you must go to **ApexKit Admin > Settings > Security** and add your Vercel domain to the **CORS Allowed Origins** list, or enable "Public API" (Allow All) so that the frontend is permitted to make the API requests.