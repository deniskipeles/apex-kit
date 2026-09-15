# Custom Frontend Authentication & Password Reset Flow

When building a headless frontend (such as a React, Vue, Next.js, or Svelte app hosted on Vercel or Netlify) that communicates with ApexKit via the JavaScript SDK (`@apexkit/sdk`), you want password reset and email verification links to point to **your frontend application** rather than the ApexKit Admin UI dashboard.

This guide details how to configure custom email templates and implement a seamless authentication flow.

---

## Step 1: Configure Email Templates in ApexKit

By default, ApexKit templates use the generic `{{link}}` variable pointing to built-in system routes. You can override these templates to point directly to your custom frontend application and pass the raw `{{token}}`.

1. Open your ApexKit **Admin Dashboard**.
2. Navigate to **Settings > Email (SMTP)**.
3. Update your **Password Reset Template** to include your frontend URL and `{{token}}`:

```text
Hello {{email}},

We received a request to reset your password for {{app_name}}. 
Click the link below to choose a new password:

https://my-custom-frontend.vercel.app/reset-password?token={{token}}

If you did not request this, please ignore this email.
```

*(Do the same for your **Email Verification Template**, pointing it to your frontend verification route, e.g., `https://my-custom-frontend.vercel.app/verify?token={{token}}`)*

---

## Step 2: User Request Flow

1. **Forgot Password:** The user visits your frontend login page (`https://my-custom-frontend.vercel.app/login`) and clicks "Forgot Password".
2. **Request Trigger:** Your frontend application invokes the SDK:
   ```typescript
   await apex.auth.requestPasswordReset("user@example.com");
   ```
3. **Email Dispatch:** ApexKit generates a secure cryptographic token and sends the email using your configured template.
4. **User Clicks Link:** The user receives the email and clicks:
   `https://my-custom-frontend.vercel.app/reset-password?token=abc-123-xyz`
5. **Frontend Landing:** Your frontend application captures the `token` parameter from the URL query string and displays a password reset form.

---

## Step 3: Handling the Reset on Your Frontend (React / Next.js Example)

Here is how you handle the confirmation step on your custom frontend using `@apexkit/sdk`:

```tsx
import React, { useState, useEffect } from 'react';
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('https://api.your-apexkit-domain.com');

export default function ResetPasswordPage() {
  const [token, setToken] = useState<string | null>(null);
  const [newPassword, setNewPassword] = useState('');
  const [status, setStatus] = useState('');
  const [loading, setLoading] = useState(false);

  // 1. Extract token from URL search query on mount
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    setToken(params.get('token'));
  }, []);

  // 2. Submit new password to ApexKit
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token) return;

    setLoading(true);
    setStatus('');

    try {
      await apex.auth.confirmPasswordReset(token, newPassword);
      setStatus('Password reset successful! Redirecting to login...');
      
      setTimeout(() => {
        window.location.href = '/login';
      }, 2000);
    } catch (err: any) {
      setStatus(`Error: ${err.message || 'The token is invalid or expired.'}`);
    } finally {
      setLoading(false);
    }
  };

  if (!token) {
    return <div className="p-8 text-center text-red-500">Invalid or missing reset token.</div>;
  }

  return (
    <div className="max-w-md mx-auto mt-20 p-6 bg-slate-800 text-white rounded-lg shadow">
      <h2 className="text-2xl font-bold mb-4">Choose a New Password</h2>
      
      <form onSubmit={handleSubmit} className="space-y-4">
        <input 
          type="password" 
          placeholder="New Password (min 6 chars)" 
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
          required
          className="w-full p-3 bg-slate-900 border border-slate-700 rounded text-white"
        />
        
        <button 
          type="submit" 
          disabled={loading}
          className="w-full p-3 bg-indigo-600 hover:bg-indigo-700 rounded font-semibold text-white"
        >
          {loading ? 'Updating...' : 'Update Password'}
        </button>
      </form>
      
      {status && <p className="mt-4 text-sm text-center text-slate-300">{status}</p>}
    </div>
  );
}
```

---

## Important CORS Configuration Note

Because your custom frontend (`https://my-custom-frontend.vercel.app`) runs on a different domain than your ApexKit backend (`https://api.your-apexkit-domain.com`), you must configure CORS:

1. Open your **ApexKit Admin Dashboard**.
2. Navigate to **Settings > Security**.
3. Add your frontend domain to the **CORS Allowed Origins** list, or enable **CORS Allow All** during development so that your frontend is permitted to execute requests against your ApexKit API.
