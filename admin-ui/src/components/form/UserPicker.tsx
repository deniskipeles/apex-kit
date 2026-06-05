import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { User, Search, X, Loader2, ChevronDown, Check } from 'lucide-react';
import { Button, Input } from './FormPrimitives';
import { usersService } from '../../features/users/services/usersService';
import { AuthUser } from '../../types';

interface UserPickerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (userId: string) => void;
  currentValue?: string;
  zIndex: number;
}

const UserPickerModal = ({
  isOpen,
  onClose,
  onSelect,
  currentValue,
  zIndex,
}: UserPickerModalProps) => {
  const [users, setUsers] = useState<AuthUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');

  useEffect(() => {
    if (isOpen) {
      setLoading(true);
      usersService.list().then((res) => {
        setUsers(res.items);
        setLoading(false);
      });
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const filtered = users.filter(
    (u) => u.email.toLowerCase().includes(search.toLowerCase()) || u.id.toString().includes(search)
  );

  return createPortal(
    <div className="fixed inset-0 flex items-center justify-center isolate" style={{ zIndex }}>
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in"
        onClick={onClose}
      />
      <div className="relative bg-background w-full h-full md:h-[80vh] md:w-[600px] md:rounded-xl border border-border shadow-2xl flex flex-col animate-in zoom-in-95">
        <div className="p-4 border-b border-border flex justify-between items-center bg-secondary/5">
          <div>
            <h3 className="font-bold flex items-center gap-2">Select User</h3>
          </div>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="h-5 w-5" />
          </Button>
        </div>
        <div className="p-4 border-b bg-secondary/10">
          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search users by email or ID..."
              className="pl-9 bg-background"
              value={search}
              onChange={(e: any) => setSearch(e.target.value)}
            />
          </div>
        </div>
        <div className="flex-1 overflow-auto p-2 space-y-1">
          {loading ? (
            <div className="flex justify-center py-8">
              <Loader2 className="animate-spin h-8 w-8 text-primary" />
            </div>
          ) : filtered.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground text-sm">No users found</div>
          ) : (
            filtered.map((user) => (
              <div
                key={user.id}
                onClick={() => {
                  onSelect(user.id);
                  onClose();
                }}
                className={`p-3 rounded-md cursor-pointer hover:bg-secondary flex justify-between items-center group ${currentValue === user.id ? 'bg-primary/10 border border-primary/20' : ''}`}
              >
                <div className="flex items-center gap-3 overflow-hidden">
                  <div className="h-8 w-8 rounded-full bg-primary/20 flex items-center justify-center text-xs font-bold text-primary shrink-0">
                    {user.email[0].toUpperCase()}
                  </div>
                  <div className="min-w-0">
                    <div className="font-medium truncate">{user.email}</div>
                    <div className="text-xs text-muted-foreground font-mono">ID: {user.id}</div>
                  </div>
                </div>
                {currentValue === user.id && <Check className="h-4 w-4 text-primary" />}
              </div>
            ))
          )}
        </div>
      </div>
    </div>,
    document.body
  );
};

interface UserPickerProps {
  value: string;
  onChange: (val: string) => void;
  depth?: number;
  label?: string;
  error?: string;
  required?: boolean;
}

export const UserPicker = ({
  value,
  onChange,
  depth = 0,
  label,
  error,
  required,
}: UserPickerProps) => {
  const [isOpen, setIsOpen] = useState(false);

  // Calculate Z-Index to be above the RecordForm panel (which starts at 70)
  const zIndex = 80 + depth * 20;

  return (
    <div className="space-y-2 w-full">
      {label && (
        <div className="text-sm font-medium flex gap-1">
          {label} {required && <span className="text-destructive">*</span>}
        </div>
      )}

      <div
        onClick={() => setIsOpen(true)}
        className={`flex h-9 w-full cursor-pointer items-center justify-between rounded-md border border-input bg-transparent px-3 text-sm hover:bg-accent hover:border-primary/50 transition-colors ${error ? 'border-destructive' : ''}`}
      >
        <div className="flex items-center gap-2 truncate">
          <User
            className={`h-3.5 w-3.5 shrink-0 ${value ? 'text-primary' : 'text-muted-foreground'}`}
          />
          <span className={`truncate ${value ? 'text-foreground' : 'text-muted-foreground'}`}>
            {value ? `User #${value}` : 'Select user...'}
          </span>
        </div>
        <ChevronDown className="h-4 w-4 opacity-50 shrink-0" />
      </div>

      {error && <span className="text-xs text-destructive">{error}</span>}

      <UserPickerModal
        isOpen={isOpen}
        onClose={() => setIsOpen(false)}
        onSelect={onChange}
        currentValue={value}
        zIndex={zIndex}
      />
    </div>
  );
};
