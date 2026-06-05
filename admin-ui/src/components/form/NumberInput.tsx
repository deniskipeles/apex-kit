import React from 'react';
import { Hash } from 'lucide-react';
import { TextInput } from './TextInput';

export const NumberInput = React.forwardRef<HTMLInputElement, any>((props, ref) => {
  return <TextInput ref={ref} type="number" icon={<Hash className="h-4 w-4" />} {...props} />;
});
NumberInput.displayName = 'NumberInput';
