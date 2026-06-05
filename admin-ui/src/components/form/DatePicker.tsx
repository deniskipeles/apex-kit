import React from 'react';
import { Calendar } from 'lucide-react';
import { TextInput } from './TextInput';

export const DatePicker = React.forwardRef<HTMLInputElement, any>((props, ref) => {
  return (
    <TextInput ref={ref} type="datetime-local" icon={<Calendar className="h-4 w-4" />} {...props} />
  );
});
DatePicker.displayName = 'DatePicker';
