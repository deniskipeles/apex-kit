
import React, { useState } from 'react';
import { Database } from 'lucide-react';
import { useAuth } from '../../../hooks/useAuth';
import { Button, Input, Label } from '../../../components/form/FormPrimitives';
import { Alert } from '../../../components/feedback/Alert';
import { apiClient } from '@/src/lib/apiClient';

export const LoginPage = () => {
  const { login } = useAuth();
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
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

  return (
    <div className="flex h-screen items-center justify-center bg-[#0f172a]">
      <div className="w-full max-w-sm space-y-6 p-6 animate-in fade-in zoom-in-95 duration-300">
        <div className="flex flex-col items-center gap-2 text-center">
          <div className="rounded-lg bg-primary/20 p-3">
             <img src={apiClient.logoUrl} alt="ApexKit Logo" className="h-8 w-auto filter invert brightness-0 saturate-100 hue-rotate-[160deg] contrast-200" style={{ filter: 'brightness(0) saturate(100%) invert(42%) sepia(91%) saturate(549%) hue-rotate(185deg) brightness(97%) contrast(92%)'}} />
          </div>
          <h1 className="text-2xl font-bold tracking-tight text-white">apexkit Admin</h1>
          <p className="text-sm text-slate-400">Enter your credentials to access the dashboard</p>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label className="text-slate-300">Email</Label>
            <Input name="email" type="email" placeholder="admin@apexkit.io" defaultValue="admin@apexkit.io" className="bg-slate-800/50 border-slate-700 text-white" />
          </div>
          <div className="space-y-2">
            <Label className="text-slate-300">Password</Label>
            <Input name="password" type="password" placeholder="••••••••" defaultValue="password" className="bg-slate-800/50 border-slate-700 text-white" />
          </div>
          {error && (
            <Alert variant="destructive">{error}</Alert>
          )}
          <Button type="submit" className="w-full" isLoading={isLoading}>Sign In</Button>
        </form>
      </div>
    </div>
  );
};
