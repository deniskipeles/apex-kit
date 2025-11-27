
import React from 'react';
import { Switch } from './FormPrimitives';

interface CheckboxProps {
  label?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  error?: string;
}

export const Checkbox = ({ label, checked, onChange, error }: CheckboxProps) => {
  return (
    <div className="flex items-center gap-3 h-9">
      <Switch checked={checked} onCheckedChange={onChange} />
      {label && <span className="text-sm font-medium">{label}</span>}
      {error && <span className="text-xs text-destructive">{error}</span>}
    </div>
  );
};
