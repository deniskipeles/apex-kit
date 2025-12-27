import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { Save, X, Shield, User, Lock, Loader2 } from 'lucide-react';
import { Button, Input, Select } from '../../../components/ui/Elements';
import { AuthUser } from '../../../types';
import { EmailInput } from '../../../components/form/EmailInput';
import { PasswordInput } from '../../../components/form/PasswordInput';
import { Alert } from '../../../components/feedback/Alert';
import { usersService } from '../services/usersService';

interface UserFormPanelProps {
  user?: AuthUser;
  onSave: (data: any) => Promise<void>;
  onCancel: () => void;
}

export const UserFormPanel = ({ user, onSave, onCancel }: UserFormPanelProps) => {
  const [formData, setFormData] = useState<any>({ email: '', password: '', role: 'user' });
  const [roles, setRoles] = useState<string[]>([]);
  const [isLoadingRoles, setIsLoadingRoles] = useState(true);
  const [isSaving, setIsSaving] = useState(false);

  // 1. Initialize Form Data
  useEffect(() => {
    if (user) {
        setFormData({ 
            email: user.email, 
            role: user.role || 'user', 
            password: '' // Keep empty for security
        });
    } else {
        setFormData({ email: '', password: '', role: 'user' });
    }
  }, [user]);

  // 2. Fetch Roles from Script
  useEffect(() => {
    const fetchRoles = async () => {
        setIsLoadingRoles(true);
        const fetchedRoles = await usersService.getRoles();
        setRoles(fetchedRoles);
        setIsLoadingRoles(false);
    };
    fetchRoles();
  }, []);
  
  const handleSubmit = async () => {
      setIsSaving(true);
      await onSave(formData);
      setIsSaving(false);
  }

  return createPortal(
    <div className="fixed inset-0 z-[100] flex justify-end isolate">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in" onClick={onCancel} />
      
      {/* Panel */}
      <div className="relative flex h-full w-full flex-col border-l border-border bg-background shadow-2xl animate-in slide-in-from-right md:max-w-lg">
        
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-5 border-b border-border bg-secondary/5">
          <div>
            <h2 className="text-xl font-bold flex items-center gap-2">
                {user ? <User className="h-5 w-5 text-primary" /> : <Shield className="h-5 w-5 text-primary" />}
                {user ? 'Edit User' : 'New User'}
            </h2>
            <p className="text-xs text-muted-foreground mt-1">
                {user ? `Update permissions and details for ${user.email}` : 'Create a new account for the dashboard.'}
            </p>
          </div>
          <Button size="icon" variant="ghost" onClick={onCancel} className="rounded-full hover:bg-secondary">
            <X className="h-5 w-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 space-y-8 overflow-y-auto p-6">
          
          {/* Section: Identity */}
          <div className="space-y-4">
              <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                  Identity
              </h3>
              <div className="grid gap-4">
                <EmailInput 
                    label="Email Address"
                    value={formData.email}
                    onChange={(e: any) => setFormData({ ...formData, email: e.target.value })}
                    required
                    autoFocus={!user}
                    disabled={!!user} // Often email is immutable ID
                    className="bg-card"
                />
                
                <div className="space-y-2">
                    <div className="flex items-center justify-between">
                        <label className="text-sm font-medium text-foreground flex items-center gap-2">
                            <Shield className="h-4 w-4" /> Role
                        </label>
                        {isLoadingRoles && <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />}
                    </div>
                    <Select
                        value={formData.role}
                        onChange={(e: any) => setFormData({ ...formData, role: e.target.value })}
                        disabled={isLoadingRoles}
                        className="bg-card"
                    >
                        {roles.map(r => (
                            <option key={r} value={r}>{r.charAt(0).toUpperCase() + r.slice(1)}</option>
                        ))}
                    </Select>
                    <p className="text-[10px] text-muted-foreground">
                        Roles define access permissions via API Policies.
                    </p>
                </div>
              </div>
          </div>

          <div className="h-px bg-border/50" />

          {/* Section: Security */}
          <div className="space-y-4">
              <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
                  Security
              </h3>
              <PasswordInput 
                label={user ? "Reset Password" : "Password"}
                value={formData.password}
                onChange={(e: any) => setFormData({ ...formData, password: e.target.value })}
                required={!user}
                placeholder={user ? "Leave blank to keep unchanged" : "••••••••"}
                className="bg-card"
              />
              
              {!user && (
                <Alert variant="default" className="bg-primary/5 border-primary/20 text-primary">
                    <Lock className="h-4 w-4 mr-2" />
                    The user will be able to change their password after first login.
                </Alert>
              )}
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-3 border-t border-border p-5 bg-secondary/5">
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} isLoading={isSaving} className="min-w-[120px] shadow-lg">
            <Save className="mr-2 h-4 w-4" /> {user ? 'Save Changes' : 'Create User'}
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
};