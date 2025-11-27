
import React from 'react';
import { Label } from './FormPrimitives';

interface FieldLabelProps extends React.LabelHTMLAttributes<HTMLLabelElement> {
  required?: boolean;
  icon?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
}

export const FieldLabel = ({ children, required, icon, className = '', ...props }: FieldLabelProps) => {
  return (
    <Label className={`flex items-center gap-2 text-sm font-medium text-muted-foreground ${className}`} {...props}>
      {icon}
      <span className="truncate">{children}</span>
      {required && <span className="text-destructive">*</span>}
    </Label>
  );
};
