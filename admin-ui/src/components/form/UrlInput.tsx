import React from 'react';
import { Link } from 'lucide-react';
import { TextInput } from './TextInput';

export const UrlInput = React.forwardRef<HTMLInputElement, any>((props, ref) => {
  return (
    <TextInput
      ref={ref}
      type="url"
      icon={<Link className="h-4 w-4" />}
      placeholder="https://example.com"
      {...props}
    />
  );
});
UrlInput.displayName = 'UrlInput';
