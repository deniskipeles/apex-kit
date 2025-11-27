import React from 'react';
import { Textarea } from './FormPrimitives';
import { FieldLabel } from './FieldLabel';

export const RichTextEditor = React.forwardRef<HTMLTextAreaElement, any>(({ label, error, required, className, ...props }, ref) => {
  return (
    <div className={`space-y-2 ${className}`}>
      {label && <FieldLabel required={required}>{label}</FieldLabel>}
      <div className="relative">
         <Textarea ref={ref} error={error} {...props} className="min-h-[150px]" />
         <div className="absolute bottom-2 right-2 text-xs text-muted-foreground pointer-events-none">Markdown Supported</div>
      </div>
    </div>
  );
});
RichTextEditor.displayName = "RichTextEditor";