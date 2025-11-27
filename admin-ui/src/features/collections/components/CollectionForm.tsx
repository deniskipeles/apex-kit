import React, { useState } from 'react';
import { Save, Shield } from 'lucide-react';
import { Button, Input, Card, CardHeader, CardTitle, CardContent, Label } from '../../../components/form/FormPrimitives';
import { SchemaEditor } from './SchemaEditor';
import { SchemaField, Collection } from '../../../types';

interface CollectionFormProps {
  onSave: (data: any) => void;
  onCancel: () => void;
  isEmbedded?: boolean;
  zIndex?: number;
  initialValues?: Collection | null;
}

export const CollectionForm = ({ onSave, onCancel, isEmbedded = false, zIndex = 0, initialValues }: CollectionFormProps) => {
  const [name, setName] = useState(initialValues?.name || '');
  const [schema, setSchema] = useState<SchemaField[]>(initialValues?.schema || [
    { name: 'title', type: 'text', required: true },
  ]);
  
  // FIX: Ensure defaults are valid policies, not empty strings
  const [rules, setRules] = useState(initialValues?.rules || { 
      read: 'public', 
      create: 'admin', 
      update: 'admin', 
      delete: 'admin' 
  });

  const effectiveZIndex = zIndex > 0 ? zIndex : 70;

  const handleSave = () => {
      // Fallback for empty strings before saving
      const cleanRules = {
          read: rules.read || 'public',
          create: rules.create || 'admin',
          update: rules.update || 'admin',
          delete: rules.delete || 'admin'
      };
      onSave({ name, schema, rules: cleanRules });
  };

  return (
    <div className={`space-y-6 ${isEmbedded ? '' : 'pb-20'}`}>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            {!isEmbedded && (
                <div>
                    <h2 className="text-3xl font-bold tracking-tight">{initialValues ? 'Edit Collection' : 'Create Collection'}</h2>
                    <p className="text-muted-foreground">{initialValues ? 'Modify schema and access rules.' : 'Define schema and rules.'}</p>
                </div>
            )}
            {isEmbedded && <div></div>}
            <div className="flex gap-3">
                <Button variant="ghost" onClick={onCancel}>Cancel</Button>
                <Button onClick={handleSave}><Save className="mr-2 h-4 w-4" /> Save Collection</Button>
            </div>
        </div>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
            <div className="lg:col-span-2 space-y-6">
                <Card>
                    <CardHeader><CardTitle>Details</CardTitle></CardHeader>
                    <CardContent className="space-y-4">
                        <div className="grid gap-2">
                            <Label>Name</Label>
                            <Input value={name} onChange={(e: any) => setName(e.target.value)} placeholder="e.g. posts" autoFocus={isEmbedded} />
                        </div>
                    </CardContent>
                </Card>
                <Card>
                    <CardHeader><CardTitle>Fields</CardTitle></CardHeader>
                    <div className="p-4 pt-0"><SchemaEditor value={schema} onChange={setSchema} zIndex={effectiveZIndex} /></div>
                </Card>
            </div>
            <div className="space-y-6">
                <Card>
                    <CardHeader><CardTitle className="flex items-center gap-2"><Shield className="h-4 w-4"/> API Rules</CardTitle></CardHeader>
                    <CardContent className="space-y-4">
                        {['read', 'create', 'update', 'delete'].map(r => (
                            <div key={r} className="space-y-1">
                                <Label className="capitalize">{r} Rule</Label>
                                <Input 
                                    value={(rules as any)[r]} 
                                    onChange={(e: any) => setRules({...rules, [r]: e.target.value})} 
                                    placeholder="public" 
                                    className="font-mono text-xs" 
                                />
                            </div>
                        ))}
                        <p className="text-[10px] text-muted-foreground pt-2">
                            Allowed: <code>public</code>, <code>auth</code>, <code>admin</code>, <code>owner:field_name</code>.
                        </p>
                    </CardContent>
                </Card>
            </div>
        </div>
    </div>
  );
};