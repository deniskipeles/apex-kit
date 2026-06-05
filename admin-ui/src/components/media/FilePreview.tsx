import React from 'react';
import { createPortal } from 'react-dom';
import { X, Download } from 'lucide-react';
import { Button } from '../form/FormPrimitives';
import { formatFileSize } from '../../lib/formatters';

interface FilePreviewProps {
  file: File;
  isOpen: boolean;
  onClose: () => void;
}

export const FilePreview = ({ file, isOpen, onClose }: FilePreviewProps) => {
  if (!isOpen) return null;

  const objectUrl = URL.createObjectURL(file);

  return createPortal(
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/80 backdrop-blur-md animate-in fade-in">
      <div className="relative max-w-4xl max-h-[90vh] w-full flex flex-col">
        <div className="flex items-center justify-between p-4 text-white">
          <div>
            <h3 className="font-bold">{file.name}</h3>
            <p className="text-sm opacity-70">{formatFileSize(file.size)}</p>
          </div>
          <div className="flex gap-2">
            <a href={objectUrl} download={file.name}>
              <Button variant="outline" size="icon">
                <Download className="h-4 w-4" />
              </Button>
            </a>
            <Button variant="outline" size="icon" onClick={onClose}>
              <X className="h-4 w-4" />
            </Button>
          </div>
        </div>
        <div className="flex-1 flex items-center justify-center p-4">
          {file.type.startsWith('image/') ? (
            <img
              src={objectUrl}
              alt={file.name}
              className="max-w-full max-h-[75vh] object-contain"
            />
          ) : (
            <div className="text-center text-white">
              <h2 className="text-2xl font-bold">Preview not available</h2>
              <p>Download the file to view its contents.</p>
            </div>
          )}
        </div>
      </div>
    </div>,
    document.body
  );
};
