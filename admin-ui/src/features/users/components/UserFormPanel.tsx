
import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { Save, X } from 'lucide-react';
import { Button, Input, Label, Separator } from '../../../components/form/FormPrimitives';
import { AdminUser } from '../../../types';
import { EmailInput } from '../../../components/form/EmailInput';
import { PasswordInput } from '../../../components/form/PasswordInput';
import { Alert } from '../../../components/feedback/Alert';

interface UserFormPanelProps {
  user?: AdminUser;
  onSave: (data: any) => Promise<void>;
  onCancel: () => void;
}

export const UserFormPanel = ({ user, onSave, onCancel }: UserFormPanelProps) => {
  const [formData, setFormData] = useState<any>({ email: '', password: '' });
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setFormData(user ? { email: user.email } : { email: '', password: '' });
  }, [user]);
  
  const handleSubmit = async () => {
      setIsSaving(true);
      await onSave(formData);
      setIsSaving(false);
  }

  return createPortal(
    <div className="fixed inset-0 z-[50] flex justify-end isolate">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in" onClick={onCancel} />
      <div className="relative flex h-full w-full flex-col border-l border-border bg-background shadow-2xl animate-in slide-in-from-right md:max-w-lg">
        <div className="flex items-center justify-between border-b p-4">
          <h2 className="text-xl font-bold">{user ? 'Edit User' : 'New User'}</h2>
          <Button size="icon" variant="ghost" onClick={onCancel}>
            <X className="h-5 w-5" />
          </Button>
        </div>
        <div className="flex-1 space-y-6 overflow-y-auto p-6">
          <EmailInput 
            label="Email Address"
            value={formData.email}
            onChange={(e: any) => setFormData({ ...formData, email: e.target.value })}
            required
            autoFocus
          />
          <PasswordInput 
            label="Password"
            value={formData.password}
            onChange={(e: any) => setFormData({ ...formData, password: e.target.value })}
            required={!user}
            placeholder={user ? "Leave blank to keep unchanged" : ""}
          />
          {!user && (
            <Alert variant="default">The user will be able to change their password after first login.</Alert>
          )}
        </div>
        <div className="flex gap-3 border-t p-4">
          <Button variant="outline" onClick={onCancel} className="flex-1">
            Cancel
          </Button>
          <Button onClick={handleSubmit} isLoading={isSaving} className="flex-1">
            <Save className="mr-2 h-4 w-4" /> Save User
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
