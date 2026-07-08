import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { Database, Search, X, Loader2, ChevronDown, Check, Plus } from 'lucide-react';
import { Button, Input } from './FormPrimitives';
import { recordsService } from '../../features/records/services/recordsService';
import { collectionsService } from '../../features/collections/services/collectionsService';
import { AppRecord, Collection } from '../../types';
import { RecordUpsertPanel } from '../../features/records/components/RecordUpsertPanel';

// Helper to intelligently resolve a descriptive label from a record's properties
const getRecordLabel = (rec: AppRecord): string => {
  if (!rec) return 'Unknown';
  
  // AppRecord properties might be flat or nested under .data
  const d = rec.data || rec;

  // 1. Try explicit descriptive fields first
  const fallbackKeys = ['title', 'name', 'subject', 'label', 'email', 'slug', 'username', 'heading'];
  for (const key of fallbackKeys) {
    if (d[key] && typeof d[key] === 'string' && d[key].trim()) {
      const valLower = d[key].trim().toLowerCase();
      if (valLower !== 'unknown' && valLower !== 'null' && valLower !== 'undefined') {
        return d[key];
      }
    }
  }

  // 2. Scan for the first reasonable string/text field (heuristic)
  // We ignore fields that look like IDs, Timestamps, raw files, or FKs to find the best descriptor
  const systemKeysPattern = /^(id|_id|uuid|created|updated|created_at|updated_at|deleted_at)$/i;
  const fileExtensionsPattern = /\.(jpg|jpeg|png|gif|svg|webp|pdf|zip|bin|txt|json)$/i;
  const dateISOStringsPattern = /^\d{4}-\d{2}-\d{2}/;

  const genericField = Object.entries(d).find(([key, val]) => {
    if (typeof val !== 'string') return false;
    const s = val.trim().slice(0,100);
    const k = key.toLowerCase();
    const sLower = s.toLowerCase();

    // Prevent matching generic system placeholders
    if (sLower === 'unknown' || sLower === 'null' || sLower === 'undefined') {
      return false;
    }

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
  return rec.id || 'Unknown';
};

interface ForeignListModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSelect: (recordId: string) => void;
  relationTo: string;
  currentValue?: string;
  depth: number;
}

const ForeignListModal = ({
  isOpen,
  onClose,
  onSelect,
  relationTo,
  currentValue,
  depth,
}: ForeignListModalProps) => {
  const [records, setRecords] = useState<AppRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [targetCollection, setTargetCollection] = useState<Collection | null>(null);

  // Ensure picker is above the form (70)
  const zIndex = 80 + depth * 20;

  useEffect(() => {
    if (isOpen && relationTo) {
      setLoading(true);
      recordsService.list(relationTo).then((res) => {
        setRecords(res.items);
        setLoading(false);
      });
      collectionsService.get(relationTo).then((c) => setTargetCollection(c || null));
    }
  }, [isOpen, relationTo]);

  if (!isOpen) return null;

  const filtered = records.filter(
    (r) =>
      r.id.toLowerCase().includes(search.toLowerCase()) ||
      JSON.stringify(r).toLowerCase().includes(search.toLowerCase())
  );

  const handleCreate = async (data: any) => {
    if (!targetCollection) return;
    const newRec = await recordsService.create(targetCollection.id, data);
    onSelect(newRec.id);
    setIsCreating(false);
    onClose();
  };

  return createPortal(
    <div className="fixed inset-0 flex items-center justify-center isolate" style={{ zIndex }}>
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in"
        onClick={onClose}
      />
      <div className="relative bg-background w-full h-full md:h-[80vh] md:w-[600px] md:rounded-xl border border-border shadow-2xl flex flex-col animate-in zoom-in-95">
        <div className="p-4 border-b border-border flex justify-between items-center bg-secondary/5">
          <div>
            <h3 className="font-bold flex items-center gap-2">
              Select Record{' '}
              <span className="text-xs font-normal text-muted-foreground font-mono px-1.5 py-0.5 rounded bg-secondary/20">
                {relationTo}
              </span>
            </h3>
          </div>
          <div className="flex gap-2">
            {targetCollection && (
              <Button size="sm" onClick={() => setIsCreating(true)}>
                <Plus className="h-4 w-4 mr-1" /> New
              </Button>
            )}
            <Button variant="ghost" size="icon" onClick={onClose}>
              <X className="h-5 w-5" />
            </Button>
          </div>
        </div>
        <div className="p-4 border-b bg-secondary/10">
          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search records..."
              className="pl-9 bg-background"
              value={search}
              onChange={(e: any) => setSearch(e.target.value)}
            />
          </div>
        </div>
        <div className="flex-1 overflow-auto p-2 space-y-1">
          {loading ? (
            <div className="flex justify-center py-8">
              <Loader2 className="animate-spin h-8 w-8 text-primary" />
            </div>
          ) : filtered.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground text-sm">No records found</div>
          ) : (
            filtered.map((rec) => (
              <div
                key={rec.id}
                onClick={() => {
                  onSelect(rec.id);
                  onClose();
                }}
                className={`p-3 rounded-md cursor-pointer hover:bg-secondary flex justify-between items-center group ${currentValue === rec.id ? 'bg-primary/10 border border-primary/20' : ''}`}
              >
                <div className="overflow-hidden">
                  <div className="font-medium truncate">
                    {getRecordLabel(rec)}
                  </div>
                  <div className="text-xs text-muted-foreground font-mono">{rec.id}</div>
                </div>
                {currentValue === rec.id && <Check className="h-4 w-4 text-primary" />}
              </div>
            ))
          )}
        </div>
        {isCreating && targetCollection && (
          <RecordUpsertPanel
            collection={targetCollection}
            onSave={handleCreate}
            onCancel={() => setIsCreating(false)}
            depth={depth + 1}
          />
        )}
      </div>
    </div>,
    document.body
  );
};

interface RelationPickerProps {
  value: string;
  onChange: (val: string) => void;
  relationTo: string;
  depth?: number;
  label?: string;
  error?: string;
}

export const RelationPicker = ({
  value,
  onChange,
  relationTo,
  depth = 0,
  label,
  error,
}: RelationPickerProps) => {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <div className="space-y-2 w-full">
      {label && <div className="text-sm font-medium">{label}</div>}
      <div
        onClick={() => setIsOpen(true)}
        className={`flex h-9 w-full cursor-pointer items-center justify-between rounded-md border border-input bg-transparent px-3 text-sm hover:bg-accent hover:border-primary/50 transition-colors ${error ? 'border-destructive' : ''}`}
      >
        <div className="flex items-center gap-2 truncate">
          <Database
            className={`h-3.5 w-3.5 shrink-0 ${value ? 'text-primary' : 'text-muted-foreground'}`}
          />
          <span className={`truncate ${value ? 'text-foreground' : 'text-muted-foreground'}`}>
            {value || 'Select record...'}
          </span>
        </div>
        <ChevronDown className="h-4 w-4 opacity-50 shrink-0" />
      </div>
      {error && <span className="text-xs text-destructive">{error}</span>}
      <ForeignListModal
        isOpen={isOpen}
        onClose={() => setIsOpen(false)}
        onSelect={onChange}
        relationTo={relationTo}
        currentValue={value}
        depth={depth}
      />
    </div>
  );
};