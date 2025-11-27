
import React, { useState, useRef } from 'react';
import { UploadCloud } from 'lucide-react';
import { Button } from '../form/FormPrimitives';

interface FileUploaderProps {
  onUpload: (files: FileList) => void;
  accept?: string;
  multiple?: boolean;
  maxSize?: number; 
}

export const FileUploader = ({ onUpload, accept, multiple = false, maxSize }: FileUploaderProps) => {
  const [isDragging, setIsDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  const handleDragLeave = () => {
    setIsDragging(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      onUpload(e.dataTransfer.files);
    }
  };

  return (
    <div
      className={`relative flex flex-col items-center justify-center rounded-lg border-2 border-dashed p-12 transition-colors ${
        isDragging
          ? 'border-primary bg-primary/5'
          : 'border-muted-foreground/25 hover:border-primary/50 hover:bg-secondary/5'
      }`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <input
        ref={inputRef}
        type="file"
        className="hidden"
        accept={accept}
        multiple={multiple}
        onChange={(e) => e.target.files && onUpload(e.target.files)}
      />
      
      <div className="flex flex-col items-center gap-4 text-center">
        <div className="rounded-full bg-primary/10 p-4">
          <UploadCloud className="h-8 w-8 text-primary" />
        </div>
        <div className="space-y-1">
          <h3 className="font-semibold tracking-tight">
            Drag & drop files here
          </h3>
          <p className="text-sm text-muted-foreground">
            or click to browse from your computer
          </p>
        </div>
        <Button 
            variant="outline" 
            size="sm" 
            onClick={() => inputRef.current?.click()}
            type="button"
        >
          Select Files
        </Button>
      </div>
    </div>
  );
};
