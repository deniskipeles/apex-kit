
import React, { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { X, Edit, Trash2, Calendar, Copy } from 'lucide-react';
import { Button, Badge, Separator } from '../../../components/form/FormPrimitives';
import { AppRecord, Collection } from '../../../types';

interface RecordPreviewProps {
  record: AppRecord | null;
  collection: Collection | null;
  isOpen: boolean;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

export const RecordPreviewPanel = ({ record, collection, isOpen, onClose, onEdit, onDelete }: RecordPreviewProps) => {
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    if (isOpen) document.addEventListener('keydown', handleEsc);
    return () => document.removeEventListener('keydown', handleEsc);
  }, [isOpen, onClose]);

  if (!isOpen || !record || !collection) return null;

  return createPortal(
    <div className="fixed inset-0 z-[50] flex justify-end isolate">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in" onClick={onClose} />
      <div className="relative w-full max-w-lg h-full bg-background border-l border-border shadow-2xl animate-in slide-in-from-right flex flex-col">
        <div className="flex items-center justify-between p-6 border-b bg-secondary/5">
           <div className="space-y-1">
              <Badge variant="outline">{collection.name}</Badge>
              <div className="font-mono font-bold text-lg select-all">{record.id}</div>
           </div>
           <Button size="icon" variant="ghost" onClick={onClose}><X className="h-5 w-5" /></Button>
        </div>
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
           <div className="grid grid-cols-2 gap-4 p-4 bg-secondary/10 rounded-lg">
              <div><span className="text-xs text-muted-foreground">Created</span><p className="text-xs font-mono">{new Date(record.created).toLocaleString()}</p></div>
              <div><span className="text-xs text-muted-foreground">Updated</span><p className="text-xs font-mono">{new Date(record.updated).toLocaleString()}</p></div>
           </div>
           <Separator />
           <div className="space-y-4">
              {collection.schema.map(field => (
                 <div key={field.name}>
                    <span className="text-sm font-semibold capitalize">{field.name}</span>
                    <div className="p-2 rounded bg-secondary/5 border text-sm break-words mt-1">
                       {typeof record[field.name] === 'object' ? JSON.stringify(record[field.name]) : String(record[field.name] ?? '-')}
                    </div>
                 </div>
              ))}
           </div>
        </div>
        <div className="p-4 border-t flex gap-3">
           <Button className="flex-1" variant="outline" onClick={onEdit}><Edit className="mr-2 h-4 w-4" /> Edit</Button>
           <Button variant="destructive" size="icon" onClick={onDelete}><Trash2 className="h-4 w-4" /></Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
