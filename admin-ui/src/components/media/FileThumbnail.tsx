import React, { useState, useEffect } from 'react';
import { FileText, FileImage, FileCode, Film, Music } from 'lucide-react';

interface FileThumbnailProps {
  file?: File;
  url?: string;
  mimeType?: string;
  name?: string;
  className?: string;
}

export const FileThumbnail = ({
  file,
  url,
  mimeType,
  name,
  className = '',
}: FileThumbnailProps) => {
  const [preview, setPreview] = useState<string | null>(null);
  const type = file?.type || mimeType || '';
  const isImage = type.startsWith('image/');

  useEffect(() => {
    if (file && isImage) {
      const reader = new FileReader();
      reader.onloadend = () => {
        setPreview(reader.result as string);
      };
      reader.readAsDataURL(file);
    } else if (url && isImage) {
      setPreview(url);
    }
  }, [file, url, isImage]);

  const getIcon = () => {
    if (type.startsWith('video/')) return <Film className="h-8 w-8 text-muted-foreground" />;
    if (type.startsWith('audio/')) return <Music className="h-8 w-8 text-muted-foreground" />;
    if (type.includes('pdf') || type.includes('document'))
      return <FileText className="h-8 w-8 text-muted-foreground" />;
    if (type.includes('json') || type.includes('xml') || type.includes('html'))
      return <FileCode className="h-8 w-8 text-muted-foreground" />;
    return <FileText className="h-8 w-8 text-muted-foreground" />;
  };

  return (
    <div
      className={`aspect-square w-full bg-secondary/50 flex items-center justify-center overflow-hidden ${className}`}
    >
      {isImage && preview ? (
        <img
          src={preview}
          alt={file?.name || name || 'Thumbnail'}
          className="h-full w-full object-cover"
          onError={() => setPreview(null)}
        />
      ) : (
        getIcon()
      )}
    </div>
  );
};
