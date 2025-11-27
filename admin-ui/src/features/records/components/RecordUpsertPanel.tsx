
import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { Save, X, Database, Search, Loader2, ChevronDown, Check, FileText, Plus } from 'lucide-react';
import { Button, Input, Label, Switch, Select } from '../../../components/form/FormPrimitives';
import { JSONEditor } from '../../../components/form/JsonEditor';
import { FileUploader } from '../../../components/media/FileUploader';
import { Collection, SchemaField, AppRecord } from '../../../types';
import { recordsService } from '../services/recordsService';

interface RecordEditorProps {
  collection: Collection;
  record?: AppRecord;
  onSave: (data: any) => Promise<void>;
  onCancel: () => void;
  depth?: number;
}

function ForeignListModal({ isOpen, onClose, onSelect, relationTo, currentValue, depth }: any) {
  const [records, setRecords] = useState<AppRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  
  // Modal must be higher than the panel (70)
  const modalZIndex = 80 + (depth * 20);

  useEffect(() => {
    if (isOpen && relationTo) {
      setLoading(true);
      recordsService.list(relationTo).then(res => {
          setRecords(res.items);
          setLoading(false);
      });
    }
  }, [isOpen, relationTo]);

  if (!isOpen) return null;

  const filteredRecords = records.filter(r => r.id.toLowerCase().includes(search.toLowerCase()) || JSON.stringify(r).toLowerCase().includes(search.toLowerCase()));

  return createPortal(
    <div className="fixed inset-0 flex items-end md:items-center justify-center isolate" style={{ zIndex: modalZIndex }}>
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in" onClick={onClose} />
      <div className="relative bg-background w-full h-[90vh] md:h-[80vh] md:w-[600px] rounded-t-xl md:rounded-xl border border-border shadow-2xl flex flex-col animate-in slide-in-from-bottom md:zoom-in-95 duration-200">
        <div className="p-4 border-b border-border flex justify-between items-center">
            <h3 className="font-bold">Select Record ({relationTo})</h3>
            <Button variant="ghost" size="icon" onClick={onClose}><X className="h-5 w-5" /></Button>
        </div>
        <div className="p-4 border-b bg-secondary/10">
            <Input placeholder="Search..." value={search} onChange={(e: any) => setSearch(e.target.value)} />
        </div>
        <div className="flex-1 overflow-auto p-2 space-y-1">
            {loading ? <Loader2 className="animate-spin mx-auto" /> : filteredRecords.map(rec => (
                <div key={rec.id} onClick={() => { onSelect(rec.id); onClose(); }} className={`p-3 rounded cursor-pointer hover:bg-secondary ${currentValue === rec.id ? 'bg-primary/10 border border-primary/20' : ''}`}>
                    <div className="font-medium">{rec.email || rec.title || rec.name || rec.id}</div>
                    <div className="text-xs text-muted-foreground">{rec.id}</div>
                </div>
            ))}
        </div>
      </div>
    </div>,
    document.body
  );
}

function RelationSelect({ value, onChange, relationTo, depth = 0 }: any) {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <div className="relative w-full">
       <div onClick={() => setIsOpen(true)} className="flex h-9 w-full cursor-pointer items-center justify-between rounded-md border border-input bg-transparent px-3 text-sm">
          <span className={value ? 'text-foreground' : 'text-muted-foreground'}>{value || 'Select record...'}</span>
          <ChevronDown className="h-4 w-4 opacity-50" />
       </div>
       <ForeignListModal isOpen={isOpen} onClose={() => setIsOpen(false)} onSelect={onChange} relationTo={relationTo} currentValue={value} depth={depth} />
    </div>
  );
}

export function RecordUpsertPanel({ collection, record, onSave, onCancel, depth = 0 }: RecordEditorProps) {
  const [formData, setFormData] = useState<any>({});
  const [isSaving, setIsSaving] = useState(false);
  // Panel starts at 70
  const zIndex = 70 + (depth * 20);

  useEffect(() => {
    if (record) {
      setFormData({ ...record });
    } else {
      const defaults: any = {};
      collection.schema.forEach(f => {
        if (f.type === 'bool') defaults[f.name] = false;
        if (f.type === 'json') defaults[f.name] = '{}';
      });
      setFormData(defaults);
    }
  }, [record, collection]);

  const renderInput = (field: SchemaField) => {
    const val = formData[field.name] ?? '';
    const setter = (v: any) => setFormData({ ...formData, [field.name]: v });

    if (field.type === 'bool') return <div className="flex items-center gap-2 h-9"><Switch checked={!!val} onCheckedChange={setter} /> <span className="text-sm">{val ? 'True' : 'False'}</span></div>;
    if (field.type === 'select') return <Select value={val} onChange={(e: any) => setter(e.target.value)}><option value="">Select...</option>{field.options?.map(o => <option key={o} value={o}>{o}</option>)}</Select>;
    if (field.type === 'json') return <JSONEditor value={typeof val === 'string' ? val : JSON.stringify(val, null, 2)} onChange={setter} height="200px" />;
    if (field.type === 'file') return <FileUploader onUpload={(files) => setter(files[0].name)} />;
    if (field.type === 'relation') return <RelationSelect value={val} onChange={setter} relationTo={field.relationTo} depth={depth} />;
    
    return <Input value={val} onChange={(e: any) => setter(e.target.value)} type={field.type === 'number' ? 'number' : 'text'} />;
  };

  return createPortal(
    <div className="fixed inset-0 flex justify-end isolate" style={{ zIndex }}>
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in" onClick={onCancel} />
      <div className="relative w-full h-full md:max-w-2xl bg-background border-l border-border shadow-2xl animate-in slide-in-from-right duration-300 flex flex-col">
        <div className="flex items-center justify-between p-4 border-b bg-secondary/5 safe-top">
          <h2 className="text-xl font-bold">{record ? 'Edit Record' : 'New Record'} <span className="text-sm text-muted-foreground ml-2 hidden sm:inline">{collection.name}</span></h2>
          <Button size="icon" variant="ghost" onClick={onCancel}><X className="h-5 w-5" /></Button>
        </div>
        <div className="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6">
           {collection.schema.map(f => (
               <div key={f.name} className="grid gap-2 md:grid-cols-4 items-start">
                   <Label className="md:col-span-1 pt-2 truncate font-semibold md:font-medium" title={f.name}>{f.name} {f.required && '*'}</Label>
                   <div className="md:col-span-3">{renderInput(f)}</div>
               </div>
           ))}
        </div>
        <div className="p-4 border-t flex gap-3 safe-bottom bg-background">
            <Button variant="outline" onClick={onCancel} className="flex-1">Cancel</Button>
            <Button onClick={() => { setIsSaving(true); onSave(formData).finally(() => setIsSaving(false)); }} isLoading={isSaving} className="flex-1"><Save className="mr-2 h-4 w-4" /> Save</Button>
        </div>
      </div>
    </div>,
    document.body
  );
}
