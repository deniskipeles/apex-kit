import React, { useState, useEffect } from 'react';
import { Braces, AlertCircle } from 'lucide-react';
import { Button, Badge } from './FormPrimitives';

interface JSONEditorProps {
  value: string;
  onChange: (value: string) => void;
  readOnly?: boolean;
  height?: string;
}

export const JSONEditor = ({
  value,
  onChange,
  readOnly = false,
  height = '400px',
}: JSONEditorProps) => {
  const [isValid, setIsValid] = useState(true);
  const [errorMsg, setErrorMsg] = useState('');
  const [internalValue, setInternalValue] = useState(value);

  useEffect(() => {
    setInternalValue(value);
    validate(value);
  }, [value]);

  const validate = (json: string) => {
    try {
      if (json.trim()) JSON.parse(json);
      setIsValid(true);
      setErrorMsg('');
    } catch (e) {
      setIsValid(false);
      setErrorMsg((e as Error).message);
    }
  };

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newVal = e.target.value;
    setInternalValue(newVal);
    validate(newVal);
    onChange(newVal);
  };

  const formatJSON = () => {
    try {
      const obj = JSON.parse(internalValue);
      const formatted = JSON.stringify(obj, null, 2);
      setInternalValue(formatted);
      onChange(formatted);
      setIsValid(true);
      setErrorMsg('');
    } catch (e) {
      // Ignore if invalid
    }
  };

  return (
    <div className="flex flex-col rounded-md border border-border overflow-hidden bg-[#1e1e1e]">
      <div className="flex items-center justify-between border-b border-border/50 bg-secondary/20 px-4 py-2">
        <div className="flex items-center gap-2">
          <Braces className="h-4 w-4 text-primary" />
          <span className="text-xs font-mono text-muted-foreground">JSON Editor</span>
          {isValid ? (
            <Badge variant="success" className="h-5 text-[10px]">
              Valid
            </Badge>
          ) : (
            <Badge variant="destructive" className="h-5 text-[10px]">
              Invalid
            </Badge>
          )}
        </div>
        <div className="flex gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-6 text-xs"
            onClick={formatJSON}
            disabled={readOnly || !isValid}
          >
            Prettify
          </Button>
        </div>
      </div>
      <div className="relative group">
        <textarea
          value={internalValue}
          onChange={handleChange}
          readOnly={readOnly}
          spellCheck={false}
          className="w-full resize-none bg-[#1e1e1e] p-4 font-mono text-sm text-[#d4d4d4] focus:outline-none"
          style={{ height }}
        />
        {errorMsg && (
          <div className="absolute bottom-0 left-0 right-0 bg-destructive/20 text-destructive text-xs p-2 border-t border-destructive/30 flex items-center gap-2">
            <AlertCircle className="h-3 w-3" />
            <span className="truncate">{errorMsg}</span>
          </div>
        )}
      </div>
    </div>
  );
};
