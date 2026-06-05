import React from 'react';
import { MoreVertical, Download, Trash2, Copy } from 'lucide-react';
import { FileThumbnail } from '../../../components/media/FileThumbnail';
import { formatFileSize } from '../../../lib/formatters';
import { StoredFile } from '../../../types';
import { Button } from '../../../components/ui/Elements';
import { APEX_FILES_THUMB_SIZE } from '@/src/constants';

interface FileCardProps {
  file: StoredFile;
  onClick: () => void;
  onDelete: (e: React.MouseEvent) => void;
}

export const FileCard: React.FC<FileCardProps> = ({ file, onClick, onDelete }) => {
  return (
    <div
      className="group relative rounded-xl border border-border bg-card overflow-hidden hover:border-primary/50 hover:shadow-sm transition-all cursor-pointer"
      onClick={onClick}
    >
      <FileThumbnail
        url={file.url + `?thumb=` + APEX_FILES_THUMB_SIZE}
        mimeType={file.mimeType}
        name={file.name}
        className="aspect-[4/3] bg-secondary/20"
      />

      <div className="p-3">
        <div className="flex justify-between items-start gap-2">
          <div className="min-w-0 flex-1">
            <h4 className="font-medium text-sm truncate" title={file.name}>
              {file.name}
            </h4>
            <p className="text-xs text-muted-foreground mt-0.5">{formatFileSize(file.size)}</p>
          </div>
          <Button
            size="icon"
            variant="ghost"
            className="h-6 w-6 -mr-1 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity"
            onClick={(e) => {
              e.stopPropagation();
              onDelete(e);
            }}
          >
            <MoreVertical className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
};
