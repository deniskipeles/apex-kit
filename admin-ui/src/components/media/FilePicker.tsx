import React, { useState, useCallback } from 'react';
import { UploadCloud, X } from 'lucide-react';
import { Button } from '../form/FormPrimitives';
import { FileThumbnail } from './FileThumbnail';
import { formatFileSize } from '../../lib/formatters';

interface FilePickerProps {
  onFilesSelected: (files: File[]) => void;
  multiple?: boolean;
  accept?: string;
}

export const FilePicker = ({ onFilesSelected, multiple = false, accept }: FilePickerProps) => {
  const [isDragging, setIsDragging] = useState(false);
  const [files, setFiles] = useState<File[]>([]);
  const inputRef = React.useRef<HTMLInputElement>(null);

  const handleFiles = useCallback(
    (selectedFiles: FileList | null) => {
      if (selectedFiles) {
        const newFiles = Array.from(selectedFiles);
        const updatedFiles = multiple ? [...files, ...newFiles] : newFiles;
        setFiles(updatedFiles);
        onFilesSelected(updatedFiles);
      }
    },
    [files, multiple, onFilesSelected]
  );

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };
  const handleDragLeave = () => setIsDragging(false);
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    handleFiles(e.dataTransfer.files);
  };

  const removeFile = (index: number) => {
    const updatedFiles = files.filter((_, i) => i !== index);
    setFiles(updatedFiles);
    onFilesSelected(updatedFiles);
  };

  return (
    <div className="space-y-4">
      <div
        className={`relative flex flex-col items-center justify-center rounded-lg border-2 border-dashed p-8 transition-colors ${
          isDragging ? 'border-primary bg-primary/5' : 'border-border hover:border-primary/50'
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
          onChange={(e) => handleFiles(e.target.files)}
        />
        <div className="flex flex-col items-center gap-2 text-center">
          <UploadCloud className="h-8 w-8 text-muted-foreground" />
          <p className="font-semibold">
            Drag & drop files or{' '}
            <span className="text-primary cursor-pointer" onClick={() => inputRef.current?.click()}>
              browse
            </span>
          </p>
          <p className="text-xs text-muted-foreground">Supports images, videos, and documents.</p>
        </div>
      </div>
      {files.length > 0 && (
        <div className="space-y-2">
          <h4 className="text-sm font-medium">Selected Files ({files.length})</h4>
          <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
            {files.map((file, index) => (
              <div
                key={index}
                className="group relative rounded-md border border-border overflow-hidden"
              >
                <FileThumbnail file={file} />
                <div className="absolute bottom-0 left-0 right-0 bg-black/50 text-white text-xs p-1.5 backdrop-blur-sm">
                  <p className="truncate font-medium">{file.name}</p>
                  <p className="opacity-70">{formatFileSize(file.size)}</p>
                </div>
                <button
                  onClick={() => removeFile(index)}
                  className="absolute -top-2 -right-2 h-6 w-6 rounded-full bg-destructive text-destructive-foreground flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
