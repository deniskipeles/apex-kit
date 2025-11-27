
import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { Save, X } from 'lucide-react';
import { Button } from '../../../components/form/FormPrimitives';
import { TextInput } from '../../../components/form/TextInput';
import { Checkbox } from '../../../components/form/Checkbox';
import { Select } from '../../../components/form/Select';
import { JSONEditor } from '../../../components/form/JsonEditor';
import { FileUploader } from '../../../components/media/FileUploader';
import { RelationPicker } from '../../../components/form/RelationPicker';
import { Collection, SchemaField, AppRecord } from '../../../types';
import { FIELD_TYPES_CONFIG } from '../../../config/field-types.config';

interface RecordFormProps {
  collection: Collection;
  record?: AppRecord;
  onSave: (data: any) => Promise<void>;
  onCancel: () => void;
  depth?: number;
}

export const RecordForm = ({ collection, record, onSave, onCancel, depth = 0 }: RecordFormProps) => {
  const [formData, setFormData] = useState<any>({});
  const [isSaving, setIsSaving] = useState(false);
  // Sidebar is 50, start at 70 to be safe
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
    const config = FIELD_TYPES_CONFIG[field.type];
    const Icon = config?.icon;

    switch (field.type) {
      case 'bool':
        return <Checkbox label={field.name} checked={!!val} onChange={setter} />;
      case 'select':
        return (
          <Select 
            label={field.name} 
            required={field.required}
            options={field.options || []} 
            value={val} 
            onChange={(e) => setter(e.target.value)} 
            icon={Icon && <Icon className="h-4 w-4"/>}
          />
        );
      case 'json':
        return (
          <div className="space-y-2">
             <div className="text-sm font-medium flex items-center gap-2">{Icon && <Icon className="h-4 w-4"/>} {field.name}</div>
             <JSONEditor value={typeof val === 'string' ? val : JSON.stringify(val, null, 2)} onChange={setter} height="200px" />
          </div>
        );
      case 'file':
        return (
          <div className="space-y-2">
            <div className="text-sm font-medium flex items-center gap-2">{Icon && <Icon className="h-4 w-4"/>} {field.name}</div>
            <FileUploader onUpload={(files) => setter(files[0].name)} />
            {val && <div className="text-xs bg-secondary/20 p-1 rounded">{val}</div>}
          </div>
        );
      case 'relation':
        return (
          <RelationPicker 
            label={field.name} 
            value={val} 
            onChange={setter} 
            relationTo={field.relationTo || ''} 
            depth={depth} 
          />
        );
      default:
        return (
          <TextInput 
            label={field.name} 
            required={field.required}
            value={val} 
            onChange={(e) => setter(e.target.value)} 
            type={field.type === 'number' ? 'number' : 'text'}
            icon={Icon && <Icon className="h-4 w-4"/>}
          />
        );
    }
  };

  return createPortal(
    <div className="fixed inset-0 flex justify-end isolate" style={{ zIndex }}>
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in" onClick={onCancel} />
      <div className="relative w-full h-full md:max-w-2xl bg-background border-l border-border shadow-2xl animate-in slide-in-from-right duration-300 flex flex-col">
        <div className="flex items-center justify-between p-4 border-b bg-secondary/5 safe-top">
          <div>
            <div className="text-xs font-bold uppercase text-muted-foreground">{collection.name}</div>
            <h2 className="text-xl font-bold">{record ? 'Edit Record' : 'New Record'}</h2>
          </div>
          <Button size="icon" variant="ghost" onClick={onCancel}><X className="h-5 w-5" /></Button>
        </div>
        <div className="flex-1 overflow-y-auto p-4 sm:p-6 space-y-6">
           {collection.schema.map(f => (
               <div key={f.name}>{renderInput(f)}</div>
           ))}
        </div>
        <div className="p-4 border-t flex gap-3 bg-background safe-bottom">
            <Button variant="outline" onClick={onCancel} className="flex-1">Cancel</Button>
            <Button onClick={() => { setIsSaving(true); onSave(formData).finally(() => setIsSaving(false)); }} isLoading={isSaving} className="flex-1"><Save className="mr-2 h-4 w-4" /> Save</Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
