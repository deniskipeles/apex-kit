import React from 'react';
import { createPortal } from 'react-dom';
import { X, Filter, Search } from 'lucide-react';
import { Button, Input, Select } from '../../../components/form/FormPrimitives';
import { Collection } from '../../../types';

interface RecordFiltersProps {
  isOpen: boolean;
  onClose: () => void;
  collection: Collection | null;
  onApplyFilters: (filters: any) => void;
}

export const RecordFilters = ({ isOpen, onClose, collection, onApplyFilters }: RecordFiltersProps) => {
  if (!isOpen) return null;

  return createPortal(
    <div className="fixed inset-0 z-40 flex justify-end isolate">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in" onClick={onClose} />
      <div className="relative w-full md:max-w-sm h-full bg-background border-l border-border shadow-2xl animate-in slide-in-from-right flex flex-col">
        <div className="flex items-center justify-between p-4 border-b">
          <h2 className="text-lg font-bold flex items-center gap-2">
            <Filter className="h-4 w-4" /> Filter Records
          </h2>
          <Button size="icon" variant="ghost" onClick={onClose}><X className="h-5 w-5" /></Button>
        </div>
        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          <div className="relative">
             <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
             <Input placeholder="Search all fields..." className="pl-9" />
          </div>

          {collection?.schema.map(field => (
            <div key={field.name} className="space-y-2">
                <label className="text-sm font-medium capitalize">{field.name}</label>
                <Input placeholder={`Filter by ${field.name}...`}/>
            </div>
          ))}

        </div>
        <div className="p-4 border-t flex gap-3">
            <Button variant="outline" className="flex-1" onClick={onClose}>Clear</Button>
            <Button className="flex-1" onClick={() => onApplyFilters({})}>Apply</Button>
        </div>
      </div>
    </div>,
    document.body
  );
};