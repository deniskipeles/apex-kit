import React, { useState } from 'react';
import { Lock, Eye, EyeOff } from 'lucide-react';
import { Input } from './FormPrimitives';
import { FieldLabel } from './FieldLabel';

export const PasswordInput = React.forwardRef<HTMLInputElement, any>(
  ({ label, error, required, className, ...props }, ref) => {
    const [show, setShow] = useState(false);
    return (
      <div className={`space-y-2 ${className}`}>
        {label && (
          <FieldLabel required={required} icon={<Lock className="h-4 w-4" />}>
            {label}
          </FieldLabel>
        )}
        <div className="relative">
          <Input
            ref={ref}
            type={show ? 'text' : 'password'}
            error={error}
            className="pr-10"
            {...props}
          />
          <button
            type="button"
            onClick={() => setShow(!show)}
            className="absolute right-3 top-2.5 text-muted-foreground hover:text-foreground transition-colors"
          >
            {show ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
          </button>
        </div>
      </div>
    );
  }
);
PasswordInput.displayName = 'PasswordInput';
