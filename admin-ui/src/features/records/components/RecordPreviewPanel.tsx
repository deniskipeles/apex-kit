import React, { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { X, Edit, Trash2, ExternalLink } from 'lucide-react';
import { Button, Badge, Separator } from '../../../components/ui/Elements';
import { AppRecord, Collection, SchemaField } from '../../../types';
import { FileThumbnail } from '../../../components/media/FileThumbnail';
import { apiClient } from '@/src/lib/apiClient';

interface RecordPreviewProps {
  record: AppRecord | null;
  collection: Collection | null;
  isOpen: boolean;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

// Helper to guess mime type for thumbnail rendering
const guessMimeType = (filename: string): string => {
  if (!filename) return 'application/octet-stream';
  const ext = filename.split('.').pop()?.toLowerCase();
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(ext || '')) return 'image/' + ext;
  if (['pdf'].includes(ext || '')) return 'application/pdf';
  return 'application/octet-stream';
};

// Helper to extract a human-readable label from an expanded record object
const getExpandedLabel = (item: any): string => {
  if (!item) return 'Unknown';
  // Check if it's a User object (flat) or a Record object (nested data)
  const data = item.data || item;

  // 1. Try explicit descriptive fields first
  const fallbackKeys = [
    'title',
    'name',
    'subject',
    'label',
    'email',
    'slug',
    'username',
    'heading',
  ];
  for (const key of fallbackKeys) {
    if (data[key] && typeof data[key] === 'string' && data[key].trim()) {
      return data[key];
    }
  }

  // 2. Scan for the first reasonable string/text field (heuristic)
  // We ignore fields that look like IDs, Timestamps, raw files, or FKs to find the best descriptor
  const systemKeysPattern = /^(id|_id|uuid|created|updated|created_at|updated_at|deleted_at)$/i;
  const fileExtensionsPattern = /\.(jpg|jpeg|png|gif|svg|webp|pdf|zip|bin|txt|json)$/i;
  const dateISOStringsPattern = /^\d{4}-\d{2}-\d{2}/;

  const genericField = Object.entries(data).find(([key, val]) => {
    if (typeof val !== 'string') return false;
    const s = val.trim().slice(0, 100);
    const k = key.toLowerCase();

    return (
      s.length > 0 &&
      !systemKeysPattern.test(key) &&
      !fileExtensionsPattern.test(s) &&
      !dateISOStringsPattern.test(s) &&
      !s.startsWith('http://') &&
      !s.startsWith('https://') &&
      !s.startsWith('data:') &&
      !k.includes('id') &&
      !k.includes('date') &&
      !k.includes('created') &&
      !k.includes('updated')
    );
  });

  if (genericField) return genericField[1] as string;

  // 3. Fallback to System ID
  return item.id || 'Unknown';
};

export const RecordPreviewPanel = ({
  record,
  collection,
  isOpen,
  onClose,
  onEdit,
  onDelete,
}: RecordPreviewProps) => {
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (isOpen) document.addEventListener('keydown', handleEsc);
    return () => document.removeEventListener('keydown', handleEsc);
  }, [isOpen, onClose]);

  if (!isOpen || !record || !collection) return null;

  const renderFieldContent = (field: SchemaField) => {
    const value = record[field.name];

    switch (field.type) {
      case 'file': {
        const filename = String(value);
        if (!filename || filename === 'null')
          return <span className="text-muted-foreground italic">No file</span>;

        return (
          <div className="flex flex-col gap-2">
            <div className="w-full aspect-video bg-black/5 rounded-md flex items-center justify-center overflow-hidden border border-border/50">
              <FileThumbnail
                url={apiClient.files.getFileUrl(filename)}
                mimeType={guessMimeType(filename)}
                className="w-full h-full object-contain"
              />
            </div>
            <div className="flex items-center justify-between gap-2">
              <div className="text-xs font-mono text-muted-foreground select-all truncate bg-background p-1.5 rounded border border-border/50 w-full">
                {filename}
              </div>
              <a
                href={apiClient.files.getFileUrl(filename)}
                target="_blank"
                rel="noreferrer"
                className="p-1.5 hover:bg-secondary rounded text-primary transition-colors"
                title="Open File"
              >
                <ExternalLink className="h-3.5 w-3.5" />
              </a>
            </div>
          </div>
        );
      }

      case 'relation':
      case 'owner': {
        const expanded = record.expand?.[field.name];
        const rawId = value;

        // 1. Array of Expanded Items
        if (Array.isArray(expanded) && expanded.length > 0) {
          return (
            <div className="flex flex-wrap gap-2">
              {expanded.map((item: any) => (
                <Badge
                  key={item.id}
                  variant="outline"
                  className="bg-background hover:bg-secondary/50 transition-colors py-1 pl-1 pr-2 gap-1.5 border-primary/20 text-foreground cursor-default"
                >
                  <div className="w-1.5 h-1.5 rounded-full bg-primary/50"></div>
                  <span className="font-medium truncate max-w-[150px]">
                    {getExpandedLabel(item)}
                  </span>
                </Badge>
              ))}
            </div>
          );
        }

        // 2. Single Expanded Item
        if (expanded && !Array.isArray(expanded)) {
          return (
            <Badge
              variant="outline"
              className="bg-background py-1 pl-1 pr-2 gap-1.5 border-primary/20 text-foreground"
            >
              <div className="w-1.5 h-1.5 rounded-full bg-primary/50"></div>
              <span className="font-medium">{getExpandedLabel(expanded)}</span>
            </Badge>
          );
        }

        // 3. Fallback to Raw ID
        return (
          <span className="font-mono text-xs text-muted-foreground">{String(rawId || '-')}</span>
        );
      }

      case 'json':
        return (
          <pre className="text-[10px] whitespace-pre-wrap font-mono custom-scrollbar max-h-60 overflow-y-auto bg-background/50 p-2 rounded">
            {typeof value === 'string' ? value : JSON.stringify(value, null, 2)}
          </pre>
        );

      case 'bool':
        return value ? (
          <Badge variant="success" className="text-[10px] px-2">
            True
          </Badge>
        ) : (
          <Badge variant="secondary" className="text-[10px] px-2 opacity-70">
            False
          </Badge>
        );

      case 'date':
        return (
          <span className="font-mono text-sm">{new Date(String(value)).toLocaleString()}</span>
        );

      default:
        // Text, String, Number, Email, Url, etc.
        return typeof value === 'object' ? (
          <pre className="text-[10px] whitespace-pre-wrap font-mono">
            {JSON.stringify(value, null, 2)}
          </pre>
        ) : (
          <span className="text-foreground/90 whitespace-pre-wrap">{String(value ?? '-')}</span>
        );
    }
  };

  return createPortal(
    <div className="fixed inset-0 z-[50] flex justify-end isolate">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in"
        onClick={onClose}
      />
      <div className="relative w-full max-w-lg h-full bg-background border-l border-border shadow-2xl animate-in slide-in-from-right flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b bg-secondary/5">
          <div className="space-y-1">
            <Badge variant="outline">{collection.name}</Badge>
            <div className="font-mono font-bold text-lg select-all">{record.id}</div>
          </div>
          <Button size="icon" variant="ghost" onClick={onClose}>
            <X className="h-5 w-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="grid grid-cols-2 gap-4 p-4 bg-secondary/10 rounded-lg border border-border">
            <div>
              <span className="text-xs text-muted-foreground uppercase tracking-wider font-semibold">
                Created
              </span>
              <p className="text-xs font-mono mt-1">{new Date(record.created).toLocaleString()}</p>
            </div>
            <div>
              <span className="text-xs text-muted-foreground uppercase tracking-wider font-semibold">
                Updated
              </span>
              <p className="text-xs font-mono mt-1">{new Date(record.updated).toLocaleString()}</p>
            </div>
            <div className="col-span-2 pt-2 border-t border-border/50">
              <span className="text-xs text-muted-foreground uppercase tracking-wider font-semibold">
                System ID
              </span>
              <p className="text-xs font-mono mt-1 select-all break-all text-muted-foreground">
                {record.id}
              </p>
            </div>
          </div>

          <Separator />

          <div className="space-y-5">
            {collection.schema.map((field) => (
              <div key={field.name} className="group">
                <div className="flex items-center gap-2 mb-2">
                  <span className="text-sm font-semibold capitalize text-foreground">
                    {field.name}
                  </span>
                  <Badge
                    variant="secondary"
                    className="text-[9px] h-4 px-1.5 uppercase font-mono text-muted-foreground"
                  >
                    {field.type}
                  </Badge>
                </div>

                <div className="p-3 rounded-lg bg-secondary/5 border border-border/50 text-sm break-words transition-colors hover:border-border">
                  {renderFieldContent(field)}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Footer Actions */}
        <div className="p-4 border-t flex gap-3 bg-background">
          <Button className="flex-1" variant="outline" onClick={onEdit}>
            <Edit className="mr-2 h-4 w-4" /> Edit Record
          </Button>
          <Button variant="destructive" size="icon" onClick={onDelete}>
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
