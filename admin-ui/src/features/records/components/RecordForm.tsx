import React from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';
import { Button } from '../../../components/ui/Elements';
import { Collection, AppRecord } from '../../../types';
import { RecordEditor } from './RecordEditor';

interface RecordFormProps {
  collection: Collection;
  record?: AppRecord;
  onSave: (data: any) => Promise<void>;
  onCancel: () => void;
  depth?: number;
}

export const RecordForm = ({ collection, record, onSave, onCancel, depth = 0 }: RecordFormProps) => {
  // Sidebar is 50, start at 70 to be safe
  const zIndex = 70 + (depth * 20);

  return createPortal(
    <div className="fixed inset-0 flex justify-end isolate" style={{ zIndex }}>
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in" onClick={onCancel} />
      <div className="relative w-full h-full md:max-w-2xl bg-background border-l border-border shadow-2xl animate-in slide-in-from-right duration-300 flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b bg-secondary/5 safe-top">
          <div>
            <div className="text-xs font-bold uppercase text-muted-foreground">{collection.name}</div>
            <h2 className="text-xl font-bold">{record ? 'Edit Record' : 'New Record'}</h2>
          </div>
          <Button size="icon" variant="ghost" onClick={onCancel}><X className="h-5 w-5" /></Button>
        </div>
        
        {/* Shared Logic Component */}
        <RecordEditor 
            collection={collection}
            record={record}
            onSave={onSave}
            onCancel={onCancel}
            depth={depth}
        />
      </div>
    </div>,
    document.body
  );
};