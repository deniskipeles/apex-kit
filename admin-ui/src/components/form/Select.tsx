import React from 'react';
import { Select as BaseSelect } from './FormPrimitives';
import { FieldLabel } from './FieldLabel';

interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  error?: string;
  icon?: React.ReactNode;
  options: string[];
}

export const Select = React.forwardRef<HTMLSelectElement, SelectProps>(
  ({ label, error, icon, required, options, className, ...props }, ref) => {
    return (
      <div className={`space-y-2 ${className}`}>
        {label && (
          <FieldLabel required={required} icon={icon}>
            {label}
          </FieldLabel>
        )}
        <BaseSelect ref={ref} error={error} {...props}>
          <option value="">Select an option</option>
          {options.map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </BaseSelect>
      </div>
    );
  }
);
Select.displayName = 'Select';
