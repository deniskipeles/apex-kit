import React, { useState, useEffect } from 'react';
import { FieldLabel } from './FieldLabel';
// Adjust this import path to where you saved the GeminiEditor.tsx
import GeminiEditor from '../../components/texteditor/components/GeminiEditor';
import { APEX_RICH_TEXT_EDITOR_DEFAULT } from '@/src/constants';

export const RichTextEditor = React.forwardRef<HTMLTextAreaElement, any>(
  ({ label, error, required, className, value, defaultValue, onChange, name, ...props }, ref) => {
    // 1. Initialize state from props (Controlled or Uncontrolled)
    // If 'value' is provided (controlled), use it. Otherwise use defaultValue or empty string.
    const defaultValueToSet =
      value && value.length > 0
        ? value
        : defaultValue && defaultValue.length > 0
          ? defaultValue
          : APEX_RICH_TEXT_EDITOR_DEFAULT;
    const [editorContent, setEditorContent] = useState<string>(defaultValueToSet);

    // 2. Sync state if the parent's value changes (e.g. data loaded from API)
    useEffect(() => {
      if (value !== undefined && value !== null && value !== '') {
        setEditorContent(value);
      }
    }, [value]);

    // 3. Handle updates from the GeminiEditor
    const handleEditorChange = (newContent: string) => {
      setEditorContent(newContent);

      // Create a synthetic event to mimic a standard Textarea change
      // This ensures compatibility with standard form handlers
      if (onChange) {
        const syntheticEvent = {
          target: {
            name: name,
            value: newContent,
            type: 'textarea',
          },
        };
        // @ts-ignore - Passing synthetic event to standard handler
        onChange(syntheticEvent);
      }
    };

    return (
      <div className={`space-y-2 ${className}`}>
        {label && <FieldLabel required={required}>{label}</FieldLabel>}

        <div className="relative group">
          {/* 
            HIDDEN INPUT: This connects the complex editor to standard HTML form logic.
            It holds the 'ref', handles 'required' validation, and stores the actual value 
            for form submission.
         */}
          <textarea
            ref={ref}
            name={name}
            value={editorContent}
            required={required}
            onChange={() => {}} // Managed by handleEditorChange
            className="sr-only" // Visually hidden but accessible to DOM logic
            tabIndex={-1}
            {...props}
          />

          {/* The Actual UI */}
          <div className={error ? 'rounded-lg border border-destructive' : ''}>
            <GeminiEditor value={editorContent} onChange={handleEditorChange} />
          </div>

          {/* Error Message Display */}
          {error && (
            <span className="text-xs text-destructive mt-1 block animate-in fade-in slide-in-from-top-1">
              {error}
            </span>
          )}

          <div className="absolute bottom-2 right-2 text-[10px] text-muted-foreground/50 pointer-events-none z-10 select-none">
            Gemini AI & Markdown Enabled
          </div>
        </div>
      </div>
    );
  }
);

RichTextEditor.displayName = 'RichTextEditor';
