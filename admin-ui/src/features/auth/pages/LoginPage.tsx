
import React, { useState, useEffect } from 'react';
import { Database, ArrowLeft, CheckCircle } from 'lucide-react';
import { useAuth } from '../../../hooks/useAuth';
import { Button, Input, Label } from '../../../components/form/FormPrimitives';
import { Alert } from '../../../components/feedback/Alert';
import { apiClient } from '@/src/lib/apiClient';

export const LoginPage = () => {
  const { login } = useAuth();
  const [mode, setMode] = useState<'login' | 'forgot' | 'reset'>('login');
  const [token, setToken] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');

  // Auto-detect secure reset token on mount from email click redirection
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const tokenParam = params.get('token');
    if (tokenParam) {
      setToken(tokenParam);
      setMode('reset');
      // Clear query params to clean up address bar beautifully
      window.history.replaceState({}, document.title, window.location.pathname);
    }
  }, []);

  const handleLoginSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    setError('');
    try {
      const form = e.target as HTMLFormElement;
      await login(form.email.value, form.password.value);
    } catch (err) {
      setError('Invalid email or password (try: admin@apexkit.io / password)');
    } finally {
      setIsLoading(false);
    }
  };

  const handleForgotSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    setError('');
    setSuccess('');
    try {
      const form = e.target as HTMLFormElement;
      const email = form.email.value;
      await apiClient.auth.requestPasswordReset(email);
      setSuccess('If the account exists, a secure password reset link has been sent to your email.');
    } catch (err: any) {
      setError(err.message || 'Failed to request password reset.');
    } finally {
      setIsLoading(false);
    }
  };

  const handleResetSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsLoading(true);
    setError('');
    setSuccess('');
    try {
      const form = e.target as HTMLFormElement;
      const password = form.password.value;
      const confirmPassword = form.confirmPassword.value;

      if (password !== confirmPassword) {
        setError('Passwords do not match.');
        setIsLoading(false);
        return;
      }

      if (password.length < 6) {
        setError('Password must be at least 6 characters.');
        setIsLoading(false);
        return;
      }

      await apiClient.auth.confirmPasswordReset(token, password);
      setSuccess('Password updated successfully! You can now sign in.');
      setMode('login');
    } catch (err: any) {
      setError(err.message || 'Failed to reset password. The link may be expired or invalid.');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="flex h-screen items-center justify-center bg-[#0f172a]">
      <div className="w-full max-w-sm space-y-6 p-6 animate-in fade-in zoom-in-95 duration-300">
        
        {/* Header Branding */}
        <div className="flex flex-col items-center gap-2 text-center">
          <div className="rounded-lg bg-primary/20 p-3">
             <img src={apiClient.logoUrl} alt="ApexKit Logo" className="h-8 w-auto filter invert brightness-0 saturate-100 hue-rotate-[160deg] contrast-200" style={{ filter: 'brightness(0) saturate(100%) invert(42%) sepia(91%) saturate(549%) hue-rotate(185deg) brightness(97%) contrast(92%)'}} />
          </div>
          <h1 className="text-2xl font-bold tracking-tight text-white">apexkit Admin</h1>
          <p className="text-sm text-slate-400">
            {mode === 'login' && 'Enter your credentials to access the dashboard'}
            {mode === 'forgot' && 'Reset your forgotten password'}
            {mode === 'reset' && 'Choose a new password for your account'}
          </p>
        </div>

        {/* Success Alert */}
        {success && (
          <Alert variant="success" className="bg-emerald-500/10 border-emerald-500/20 text-emerald-400 flex items-start">
            <CheckCircle className="h-4 w-4 mr-2 shrink-0 mt-0.5" />
            <span>{success}</span>
          </Alert>
        )}

        {/* Error Alert */}
        {error && (
          <Alert variant="destructive">{error}</Alert>
        )}

        {/* --- SIGN IN MODE --- */}
        {mode === 'login' && (
          <form onSubmit={handleLoginSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label className="text-slate-300">Email</Label>
              <Input name="email" type="email" placeholder="admin@apexkit.io" defaultValue="admin@apexkit.io" required className="bg-slate-800/50 border-slate-700 text-white" />
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-slate-300">Password</Label>
                <button 
                  type="button" 
                  onClick={() => { setMode('forgot'); setError(''); setSuccess(''); }} 
                  className="text-xs text-primary hover:underline font-semibold"
                >
                  Forgot password?
                </button>
              </div>
              <Input name="password" type="password" placeholder="••••••••" defaultValue="password" required className="bg-slate-800/50 border-slate-700 text-white" />
            </div>
            <Button type="submit" className="w-full" isLoading={isLoading}>Sign In</Button>
          </form>
        )}

        {/* --- FORGOT PASSWORD MODE --- */}
        {mode === 'forgot' && (
          <form onSubmit={handleForgotSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label className="text-slate-300">Email Address</Label>
              <Input name="email" type="email" placeholder="admin@apexkit.io" required className="bg-slate-800/50 border-slate-700 text-white" />
            </div>
            <Button type="submit" className="w-full" isLoading={isLoading}>Send Reset Link</Button>
            <button 
              type="button" 
              onClick={() => { setMode('login'); setError(''); setSuccess(''); }} 
              className="w-full flex items-center justify-center gap-2 text-xs text-slate-400 hover:text-white transition-colors"
            >
              <ArrowLeft className="h-3 w-3" /> Back to Sign In
            </button>
          </form>
        )}

        {/* --- CONFIRM RESET MODE --- */}
        {mode === 'reset' && (
          <form onSubmit={handleResetSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label className="text-slate-300">New Password</Label>
              <Input name="password" type="password" placeholder="••••••••" required className="bg-slate-800/50 border-slate-700 text-white" />
            </div>
            <div className="space-y-2">
              <Label className="text-slate-300">Confirm New Password</Label>
              <Input name="confirmPassword" type="password" placeholder="••••••••" required className="bg-slate-800/50 border-slate-700 text-white" />
            </div>
            <Button type="submit" className="w-full" isLoading={isLoading}>Update Password</Button>
            <button 
              type="button" 
              onClick={() => { setMode('login'); setError(''); setSuccess(''); }} 
              className="w-full flex items-center justify-center gap-2 text-xs text-slate-400 hover:text-white transition-colors"
            >
              <ArrowLeft className="h-3 w-3" /> Cancel & Sign In
            </button>
          </form>
        )}

      </div>
    </div>
  );
};
