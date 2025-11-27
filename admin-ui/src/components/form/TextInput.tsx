
import React from 'react';
import { Input } from './FormPrimitives';
import { FieldLabel } from './FieldLabel';

interface TextInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  icon?: React.ReactNode;
}

export const TextInput = React.forwardRef<HTMLInputElement, TextInputProps>(({ label, error, icon, required, className, ...props }, ref) => {
  return (
    <div className={`space-y-2 ${className}`}>
      {label && <FieldLabel required={required} icon={icon}>{label}</FieldLabel>}
      <Input ref={ref} error={error} {...props} />
    </div>
  );
});
TextInput.displayName = 'TextInput';
