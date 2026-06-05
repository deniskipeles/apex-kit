import React from 'react';
import { Mail } from 'lucide-react';
import { TextInput } from './TextInput';

export const EmailInput = React.forwardRef<HTMLInputElement, any>((props, ref) => {
  return <TextInput ref={ref} type="email" icon={<Mail className="h-4 w-4" />} {...props} />;
});
EmailInput.displayName = 'EmailInput';
