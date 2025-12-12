import React, { useState } from 'react';
import { Save, Shield, Fingerprint, Plus, Trash2, X } from 'lucide-react';
import { Button, Input, Card, CardHeader, CardTitle, CardContent, Label, Badge, Checkbox } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog'; // Assuming you have a Dialog component
import { SchemaEditor } from './SchemaEditor';
import { SchemaField, Collection } from '../../../types';

interface CollectionFormProps {
  onSave: (data: any) => void;
  onCancel: () => void;
  isEmbedded?: boolean;
  zIndex?: number;
  initialValues?: Collection | null;
}

// --- Helper Component for the Index Modal ---
const CompositeIndexModal = ({ 
    isOpen, 
    onClose, 
    fields, 
    onAdd 
}: { 
    isOpen: boolean; 
    onClose: () => void; 
    fields: SchemaField[]; 
    onAdd: (fields: string[]) => void;
}) => {
    const [selected, setSelected] = useState<Set<string>>(new Set());

    const toggleField = (name: string) => {
        const next = new Set(selected);
        if (next.has(name)) next.delete(name);
        else next.add(name);
        setSelected(next);
    };

    const handleSave = () => {
        onAdd(Array.from(selected));
        setSelected(new Set());
        onClose();
    };

    if (!isOpen) return null;

    return (
        <Dialog isOpen={isOpen} onClose={onClose} title="Create Composite Unique Index" size="md">
            <div className="space-y-4">
                <p className="text-sm text-muted-foreground">
                    Select the fields that should form a unique combination. Rows with duplicate values across <em>all</em> these fields will be rejected.
                </p>
                <div className="max-h-[300px] overflow-y-auto border border-border rounded-md p-2 space-y-1">
                    {fields.map(field => (
                        <div 
                            key={field.name} 
                            className={`flex items-center gap-3 p-2 rounded hover:bg-secondary/50 cursor-pointer ${selected.has(field.name) ? 'bg-primary/10' : ''}`}
                            onClick={() => toggleField(field.name)}
                        >
                            <Checkbox checked={selected.has(field.name)} readOnly />
                            <div className="flex-1">
                                <span className="font-medium text-sm">{field.name}</span>
                                <span className="ml-2 text-xs text-muted-foreground uppercase">{field.type}</span>
                            </div>
                        </div>
                    ))}
                    {fields.length === 0 && <div className="text-sm text-muted-foreground p-2">No fields defined yet.</div>}
                </div>
                <div className="flex justify-end gap-2 pt-2">
                    <Button variant="ghost" onClick={onClose}>Cancel</Button>
                    <Button onClick={handleSave} disabled={selected.size < 2}>Create Index</Button>
                </div>
            </div>
        </Dialog>
    );
};

export const CollectionForm = ({ onSave, onCancel, isEmbedded = false, zIndex = 0, initialValues }: CollectionFormProps) => {
  const [name, setName] = useState(initialValues?.name || '');
  const [schema, setSchema] = useState<SchemaField[]>(initialValues?.schema || [
    { name: 'title', type: 'string', required: true, indexed: true, uid: Math.floor(Math.random() * 0xFFFFFFFF).toString(16), position: 0 },
  ]);
  
  // State for Composite Indexes (Array of Arrays of strings)
  const [compositeUnique, setCompositeUnique] = useState<string[][]>(initialValues?.compositeUnique || []);
  const [isIndexModalOpen, setIsIndexModalOpen] = useState(false);

  // Rules State
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

      // Combine everything
      onSave({ 
          name, 
          schema, 
          rules: cleanRules,
          compositeUnique // <--- Pass the new indexes to parent
      });
  };

  const addCompositeIndex = (fields: string[]) => {
      setCompositeUnique([...compositeUnique, fields]);
  };

  const removeCompositeIndex = (index: number) => {
      const next = [...compositeUnique];
      next.splice(index, 1);
      setCompositeUnique(next);
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
                {/* 1. Details */}
                <Card>
                    <CardHeader><CardTitle>Details</CardTitle></CardHeader>
                    <CardContent className="space-y-4">
                        <div className="grid gap-2">
                            <Label>Name</Label>
                            <Input value={name} onChange={(e: any) => setName(e.target.value)} placeholder="e.g. posts" autoFocus={isEmbedded} />
                        </div>
                    </CardContent>
                </Card>

                {/* 2. Fields */}
                <Card>
                    <CardHeader><CardTitle>Fields</CardTitle></CardHeader>
                    <div className="p-4 pt-0"><SchemaEditor value={schema} onChange={setSchema} zIndex={effectiveZIndex} /></div>
                </Card>

                {/* 3. NEW: Composite Indexes */}
                <Card>
                    <CardHeader className="flex flex-row items-center justify-between pb-2">
                        <CardTitle className="flex items-center gap-2">
                            <Fingerprint className="h-4 w-4" /> Composite Indexes
                        </CardTitle>
                        <Button size="sm" variant="outline" onClick={() => setIsIndexModalOpen(true)}>
                            <Plus className="h-3 w-3 mr-1" /> Add Index
                        </Button>
                    </CardHeader>
                    <CardContent>
                        {compositeUnique.length === 0 ? (
                            <p className="text-sm text-muted-foreground italic">No composite unique constraints defined.</p>
                        ) : (
                            <div className="space-y-2">
                                {compositeUnique.map((group, idx) => (
                                    <div key={idx} className="flex items-center justify-between p-2 rounded border border-border bg-card">
                                        <div className="flex flex-wrap gap-1.5 items-center">
                                            <span className="text-xs font-bold text-muted-foreground mr-1">UNIQUE</span>
                                            <span className="text-xs text-muted-foreground">(</span>
                                            {group.map((f, i) => (
                                                <React.Fragment key={i}>
                                                    <Badge variant="secondary" className="font-mono text-[10px]">{f}</Badge>
                                                    {i < group.length - 1 && <span className="text-muted-foreground text-xs">+</span>}
                                                </React.Fragment>
                                            ))}
                                            <span className="text-xs text-muted-foreground">)</span>
                                        </div>
                                        <Button size="icon" variant="ghost" className="h-7 w-7 text-muted-foreground hover:text-destructive" onClick={() => removeCompositeIndex(idx)}>
                                            <Trash2 className="h-3.5 w-3.5" />
                                        </Button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </CardContent>
                </Card>
            </div>

            {/* Sidebar: Rules */}
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
                        <div className="text-[10px] text-muted-foreground pt-2 bg-secondary/10 p-2 rounded">
                            <p className="font-semibold mb-1">Quick Ref:</p>
                            <ul className="space-y-1">
                                <li><code>public</code> - Everyone</li>
                                <li><code>auth</code> - Logged in</li>
                                <li><code>admin</code> - Admins only</li>
                                <li><code>owner:field</code> - Match User ID</li>
                            </ul>
                        </div>
                    </CardContent>
                </Card>
            </div>
        </div>

        {/* Index Creator Modal */}
        <CompositeIndexModal 
            isOpen={isIndexModalOpen} 
            onClose={() => setIsIndexModalOpen(false)} 
            fields={schema}
            onAdd={addCompositeIndex}
        />
    </div>
  );
};