
import React, { useState, useEffect, Suspense } from 'react';
import { 
  Plus, Trash2, Settings, GripVertical, Type, Hash, ToggleLeft, 
  Mail, Link, Calendar, List, FileJson, File, Database, Check, X,
  Search, ShieldCheck, ChevronDown, Loader2
} from 'lucide-react';
import { Button, Input, Switch, Label, Badge, Textarea } from '../../../components/form/FormPrimitives';
import { Dialog } from '../../../components/ui/Dialog';
import { SchemaField, Collection, CollectionType } from '../../../types';
import { collectionsService } from '../services/collectionsService';

// Lazy load CollectionForm to avoid circular dependency issues
const CollectionForm = React.lazy(() => 
  import('./CollectionForm').then(module => ({ default: module.CollectionForm }))
);

interface SchemaBuilderProps {
  value: SchemaField[];
  onChange: (fields: SchemaField[]) => void;
  zIndex?: number;
}

const TYPE_CONFIG: Record<string, { icon: any; label: string; color: string; description: string }> = {
  text: { icon: Type, label: 'Text', color: 'text-blue-500', description: 'Small to long strings' },
  number: { icon: Hash, label: 'Number', color: 'text-orange-500', description: 'Integers or floats' },
  bool: { icon: ToggleLeft, label: 'Boolean', color: 'text-green-500', description: 'True/false toggle' },
  email: { icon: Mail, label: 'Email', color: 'text-purple-500', description: 'Email validation' },
  url: { icon: Link, label: 'URL', color: 'text-cyan-500', description: 'Link validation' },
  date: { icon: Calendar, label: 'Date', color: 'text-pink-500', description: 'Date & time picker' },
  select: { icon: List, label: 'Select', color: 'text-yellow-500', description: 'Dropdown options' },
  json: { icon: FileJson, label: 'JSON', color: 'text-red-500', description: 'Structured data' },
  file: { icon: File, label: 'File', color: 'text-gray-500', description: 'File uploads' },
  relation: { icon: Database, label: 'Relation', color: 'text-emerald-500', description: 'Link to another record' },
};

// --- Helper Components ---

const ValidationPreview = ({ field }: { field: SchemaField }) => {
    const [testValue, setTestValue] = useState('');
    const [status, setStatus] = useState<'idle' | 'valid' | 'invalid'>('idle');
    const [error, setError] = useState('');

    useEffect(() => {
        if (!testValue) {
            setStatus('idle');
            setError('');
            return;
        }
        
        let isValid = true;
        let errMsg = '';

        if (field.pattern && (field.type === 'text' || field.type === 'email' || field.type === 'url')) {
            try {
                const regex = new RegExp(field.pattern);
                if (!regex.test(testValue)) {
                    isValid = false;
                    errMsg = 'Does not match regex pattern';
                }
            } catch (e) {
                isValid = false;
                errMsg = 'Invalid regex pattern';
            }
        }

        if (field.minLength && testValue.length < field.minLength) {
             isValid = false; errMsg = `Too short (min ${field.minLength})`;
        }
        if (field.maxLength && testValue.length > field.maxLength) {
             isValid = false; errMsg = `Too long (max ${field.maxLength})`;
        }

        if (field.type === 'number') {
            const num = Number(testValue);
            if (isNaN(num)) {
                isValid = false; errMsg = 'Not a number';
            } else {
                if (field.min !== null && field.min !== undefined && num < field.min) {
                    isValid = false; errMsg = `Less than min (${field.min})`;
                }
                if (field.max !== null && field.max !== undefined && num > field.max) {
                    isValid = false; errMsg = `Greater than max (${field.max})`;
                }
            }
        }

        setStatus(isValid ? 'valid' : 'invalid');
        setError(errMsg);

    }, [testValue, field]);

    return (
        <div className="rounded-md bg-secondary/10 border border-border p-3 space-y-2">
            <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                <ShieldCheck className="h-3.5 w-3.5" /> Validation Preview
            </div>
            <div className="flex gap-2">
                <Input 
                    value={testValue} 
                    onChange={(e: any) => setTestValue(e.target.value)} 
                    placeholder={`Test ${field.type} validation...`}
                    className={`h-8 text-sm ${status === 'valid' ? 'border-emerald-500 focus-visible:ring-emerald-500' : status === 'invalid' ? 'border-destructive focus-visible:ring-destructive' : ''}`}
                />
                {status === 'valid' && <div className="flex items-center justify-center h-8 w-8 rounded bg-emerald-500/10 text-emerald-500"><Check className="h-4 w-4" /></div>}
                {status === 'invalid' && <div className="flex items-center justify-center h-8 w-8 rounded bg-destructive/10 text-destructive" title={error}><X className="h-4 w-4" /></div>}
            </div>
            {status === 'invalid' && <p className="text-[10px] text-destructive">{error}</p>}
        </div>
    );
};

const CollectionPickerDialog = ({ isOpen, onClose, onSelect, onCreate, zIndex }: { isOpen: boolean, onClose: () => void, onSelect: (id: string) => void, onCreate: () => void, zIndex: number }) => {
    const [search, setSearch] = useState('');
    const [collections, setCollections] = useState<Collection[]>([]);
    const [loading, setLoading] = useState(false);

    useEffect(() => {
        if (isOpen) {
            setLoading(true);
            collectionsService.list().then(res => {
                setCollections(res);
                setLoading(false);
            });
        }
    }, [isOpen]);

    const filtered = collections.filter(c => c.name.toLowerCase().includes(search.toLowerCase()));

    return (
        <Dialog isOpen={isOpen} onClose={onClose} title="Select Related Collection" size="sm" zIndex={zIndex}>
            <div className="space-y-4">
                <div className="relative">
                    <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input 
                        placeholder="Search collections..." 
                        className="pl-9" 
                        value={search}
                        onChange={(e: any) => setSearch(e.target.value)}
                        autoFocus
                    />
                </div>
                <div className="border rounded-md divide-y divide-border max-h-[300px] overflow-y-auto bg-card">
                    {loading ? (
                        <div className="p-4 text-center text-xs text-muted-foreground">Loading...</div>
                    ) : filtered.length === 0 ? (
                        <div className="p-8 text-center text-muted-foreground text-sm">No collections found.</div>
                    ) : (
                        filtered.map(c => (
                            <button
                                key={c.id}
                                className="w-full flex items-center justify-between px-4 py-3 text-sm hover:bg-accent transition-colors text-left group"
                                onClick={() => onSelect(c.id)}
                            >
                                <div className="flex items-center gap-2">
                                    <Database className="h-4 w-4 text-muted-foreground group-hover:text-primary" />
                                    <span className="font-medium">{c.name}</span>
                                </div>
                                <Badge variant="secondary" className="text-[10px] group-hover:bg-background uppercase">{c.type}</Badge>
                            </button>
                        ))
                    )}
                </div>
                <Button className="w-full" variant="outline" onClick={onCreate}>
                    <Plus className="mr-2 h-4 w-4" /> Create New Collection
                </Button>
            </div>
        </Dialog>
    );
};

// --- Main Field Editor Dialog ---

const FieldEditorDialog = ({ field, onSave, onCancel, isOpen, isNew, zIndex }: { field: SchemaField, onSave: (f: SchemaField) => void, onCancel: () => void, isOpen: boolean, isNew: boolean, zIndex: number }) => {
  const [data, setData] = useState<SchemaField>(field);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [selectOptions, setSelectOptions] = useState(field.options?.join('\n') || '');
  
  // Relation Picker State
  const [isColPickerOpen, setIsColPickerOpen] = useState(false);
  const [isCollectionCreatorOpen, setIsCollectionCreatorOpen] = useState(false);

  useEffect(() => {
    setData(field);
    setSelectOptions(field.options?.join('\n') || '');
  }, [field]);

  // Load collections just for displaying the name of the selected relation
  useEffect(() => {
    if (isOpen) {
        collectionsService.list().then(setCollections);
    }
  }, [isOpen]);

  const handleSave = () => {
    const updated = { ...data };
    if (data.type === 'select') {
        updated.options = selectOptions.split('\n').map(s => s.trim()).filter(s => s);
    }
    onSave(updated);
  };

  const handleCreateCollection = async (collectionData: any) => {
      try {
          const newCol = await collectionsService.create(collectionData);
          setCollections(prev => [...prev, newCol]);
          setData(prev => ({ ...prev, relationTo: newCol.id }));
          setIsCollectionCreatorOpen(false);
          setIsColPickerOpen(false);
      } catch (e) {
          console.error(e);
      }
  };

  return (
    <Dialog isOpen={isOpen} onClose={onCancel} title={isNew ? 'Add New Field' : 'Edit Field'} size="lg" zIndex={zIndex}>
      <div className="space-y-6 pb-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
            {/* Left Column: Identity & Type */}
            <div className="space-y-6">
                <div className="space-y-4 p-4 rounded-xl border border-border bg-secondary/5">
                    <div className="space-y-2">
                        <Label required>Field Name</Label>
                        <Input 
                            value={data.name} 
                            onChange={(e: any) => setData({ ...data, name: e.target.value })}
                            placeholder="e.g. first_name"
                            className="font-mono"
                            autoFocus
                        />
                        <p className="text-[10px] text-muted-foreground">Use lowercase, no spaces (a-z, 0-9, _)</p>
                    </div>

                    <div className="space-y-2">
                        <Label>Data Type</Label>
                        <div className="grid grid-cols-2 gap-2">
                            {Object.entries(TYPE_CONFIG).map(([key, config]) => (
                                <button
                                    key={key}
                                    type="button"
                                    onClick={() => setData({ ...data, type: key as any })}
                                    className={`flex flex-col items-start p-2.5 rounded-lg border transition-all text-left ${data.type === key ? 'border-primary bg-primary/10 ring-1 ring-primary' : 'border-border hover:bg-secondary/50 hover:border-primary/30'}`}
                                >
                                    <div className="flex items-center gap-2 mb-1">
                                        <config.icon className={`h-4 w-4 ${data.type === key ? 'text-primary' : 'text-muted-foreground'}`} />
                                        <span className={`text-xs font-semibold ${data.type === key ? 'text-foreground' : 'text-muted-foreground'}`}>{config.label}</span>
                                    </div>
                                </button>
                            ))}
                        </div>
                    </div>
                </div>
                
                {/* Validation Preview */}
                {(data.type === 'text' || data.type === 'number' || data.type === 'email' || data.type === 'url') && (
                    <ValidationPreview field={data} />
                )}
            </div>

            {/* Right Column: Detailed Configuration */}
            <div className="space-y-6">
                 <div className="rounded-xl border border-border bg-card p-5 space-y-5 shadow-sm">
                    <div className="flex items-center justify-between border-b border-border pb-2">
                        <div className="flex items-center gap-2">
                            <Settings className="h-4 w-4 text-muted-foreground" />
                            <h3 className="text-sm font-semibold">Settings</h3>
                        </div>
                        <Badge variant="outline" className="capitalize">{TYPE_CONFIG[data.type].label}</Badge>
                    </div>

                    {/* Common Toggles */}
                    <div className="grid grid-cols-2 gap-4">
                        <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                            <div className="space-y-0.5">
                                <Label className="text-xs cursor-pointer" onClick={() => setData({...data, required: !data.required})}>Required</Label>
                            </div>
                            <Switch checked={data.required} onCheckedChange={(c) => setData({ ...data, required: c })} />
                        </div>
                         <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                            <div className="space-y-0.5">
                                <Label className="text-xs cursor-pointer" onClick={() => setData({...data, unique: !data.unique})}>Unique</Label>
                            </div>
                            <Switch checked={data.unique} onCheckedChange={(c) => setData({ ...data, unique: c })} />
                        </div>
                        {/* --- ADD THIS BLOCK START --- */}
                        {(data.type === 'text' || data.type === 'email' || data.type === 'url') && (
                            <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                                <div className="space-y-0.5">
                                    <Label className="text-xs cursor-pointer" onClick={() => setData({...data, indexed: !data.indexed})}>Indexed</Label>
                                    <p className="text-[8px] text-muted-foreground">Enable Instant Search</p>
                                </div>
                                <Switch checked={data.indexed} onCheckedChange={(c) => setData({ ...data, indexed: c })} />
                            </div>
                        )}
                    </div>
                    
                    {/* Default Value */}
                    {data.type !== 'file' && data.type !== 'relation' && data.type !== 'json' && (
                        <div className="space-y-2">
                            <Label>Default Value</Label>
                            <Input 
                                value={data.default || ''} 
                                onChange={(e: any) => setData({ ...data, default: e.target.value })}
                                placeholder={data.type === 'bool' ? 'true/false' : 'Optional default...'}
                                className="h-8 text-sm"
                            />
                        </div>
                    )}

                    {/* Type Specific Configs */}
                    <div className="space-y-4 animate-in fade-in duration-300">
                        
                        {/* SELECT OPTIONS */}
                        {data.type === 'select' && (
                            <div className="space-y-2">
                                <Label>Options (one per line)</Label>
                                <Textarea 
                                    value={selectOptions} 
                                    onChange={(e: any) => setSelectOptions(e.target.value)} 
                                    placeholder="Option A&#10;Option B&#10;Option C"
                                    className="min-h-[100px] font-mono text-xs"
                                />
                            </div>
                        )}

                        {/* NUMBER CONSTRAINTS */}
                        {data.type === 'number' && (
                            <div className="grid grid-cols-2 gap-3">
                                <div className="space-y-1">
                                    <Label className="text-xs">Min Value</Label>
                                    <Input type="number" value={data.min ?? ''} onChange={(e: any) => setData({...data, min: e.target.value ? Number(e.target.value) : null})} className="h-8" />
                                </div>
                                 <div className="space-y-1">
                                    <Label className="text-xs">Max Value</Label>
                                    <Input type="number" value={data.max ?? ''} onChange={(e: any) => setData({...data, max: e.target.value ? Number(e.target.value) : null})} className="h-8" />
                                </div>
                            </div>
                        )}

                        {/* TEXT CONSTRAINTS */}
                        {(data.type === 'text' || data.type === 'email' || data.type === 'url' || data.type === 'json') && (
                             <div className="space-y-3">
                                {data.type !== 'json' && (
                                    <div className="space-y-1">
                                        <Label className="text-xs">Regex Pattern</Label>
                                        <Input 
                                            value={data.pattern || ''} 
                                            onChange={(e: any) => setData({...data, pattern: e.target.value})} 
                                            className="h-8 font-mono text-xs" 
                                            placeholder="e.g. ^[A-Z]+$"
                                        />
                                    </div>
                                )}
                                <div className="grid grid-cols-2 gap-3">
                                    <div className="space-y-1">
                                        <Label className="text-xs">{data.type === 'json' ? 'Max Size (chars)' : 'Min Length'}</Label>
                                        <Input type="number" value={data.minLength ?? ''} onChange={(e: any) => setData({...data, minLength: e.target.value ? Number(e.target.value) : null})} className="h-8" />
                                    </div>
                                    <div className="space-y-1">
                                        <Label className="text-xs">{data.type === 'json' ? 'N/A' : 'Max Length'}</Label>
                                        <Input type="number" value={data.maxLength ?? ''} onChange={(e: any) => setData({...data, maxLength: e.target.value ? Number(e.target.value) : null})} className="h-8" disabled={data.type === 'json'} />
                                    </div>
                                </div>
                             </div>
                        )}

                         {/* FILE CONSTRAINTS */}
                         {data.type === 'file' && (
                             <div className="space-y-3">
                                <div className="space-y-1">
                                    <Label className="text-xs">Max Size (bytes)</Label>
                                    <Input type="number" value={data.maxSize ?? ''} onChange={(e: any) => setData({...data, maxSize: e.target.value ? Number(e.target.value) : null})} className="h-8" placeholder="5242880 (5MB)" />
                                </div>
                                <div className="space-y-1">
                                    <Label className="text-xs">Allowed MIME Types</Label>
                                    <Input value={data.mimeTypes?.join(', ') || ''} onChange={(e: any) => setData({...data, mimeTypes: e.target.value.split(',').map((t:string) => t.trim())})} className="h-8" placeholder="image/*, application/pdf" />
                                </div>
                             </div>
                        )}

                        {/* RELATION PICKER */}
                        {data.type === 'relation' && (
                             <div className="space-y-2">
                                <Label required>Related Collection</Label>
                                <div className="relative">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        className="w-full justify-between text-left font-normal px-3 h-9"
                                        onClick={() => setIsColPickerOpen(true)}
                                    >
                                        {data.relationTo ? (
                                            <span className="flex items-center gap-2">
                                                <Database className="h-3.5 w-3.5 text-primary" />
                                                <span className="font-semibold text-foreground">{collections.find(c => c.id === data.relationTo || c.name === data.relationTo)?.name || data.relationTo}</span>
                                            </span>
                                        ) : (
                                            <span className="text-muted-foreground">Select Collection...</span>
                                        )}
                                        <ChevronDown className="h-4 w-4 opacity-50" />
                                    </Button>

                                    <CollectionPickerDialog 
                                        isOpen={isColPickerOpen}
                                        onClose={() => setIsColPickerOpen(false)}
                                        onSelect={(id) => {
                                            setData({ ...data, relationTo: id });
                                            setIsColPickerOpen(false);
                                        }}
                                        onCreate={() => {
                                            setIsColPickerOpen(false);
                                            setIsCollectionCreatorOpen(true);
                                        }}
                                        zIndex={zIndex + 10}
                                    />

                                     {/* Recursively Create New Collection */}
                                    <Dialog 
                                        isOpen={isCollectionCreatorOpen} 
                                        onClose={() => setIsCollectionCreatorOpen(false)} 
                                        size="xl" 
                                        title="New Collection" 
                                        zIndex={zIndex + 20}
                                    >
                                        <Suspense fallback={<div className="flex justify-center p-8"><Loader2 className="animate-spin" /></div>}>
                                            <CollectionForm 
                                                onSave={handleCreateCollection}
                                                onCancel={() => setIsCollectionCreatorOpen(false)}
                                                isEmbedded
                                                zIndex={zIndex + 20}
                                            />
                                        </Suspense>
                                    </Dialog>
                                </div>
                            </div>
                        )}
                    </div>
                 </div>
            </div>
        </div>

        <div className="flex justify-end gap-3 pt-6 border-t border-border mt-4">
            <Button variant="ghost" onClick={onCancel}>Cancel</Button>
            <Button onClick={handleSave} disabled={!data.name}><Check className="mr-2 h-4 w-4" /> {isNew ? 'Add Field' : 'Save Changes'}</Button>
        </div>
      </div>
    </Dialog>
  );
};

// --- Main Component ---

export const SchemaEditor = ({ value, onChange, zIndex = 60 }: SchemaBuilderProps) => {
  const [editingIdx, setEditingIdx] = useState<number | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);

  const handleSaveField = (field: SchemaField) => {
      const newFields = [...value];
      if (isCreating) {
          newFields.push(field);
      } else if (editingIdx !== null) {
          newFields[editingIdx] = field;
      }
      onChange(newFields);
      setEditingIdx(null);
      setIsCreating(false);
  };

  const removeField = (index: number, e: React.MouseEvent) => {
      e.stopPropagation();
      const newFields = [...value];
      newFields.splice(index, 1);
      onChange(newFields);
  };
  
  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = "move";
  };

  const handleDrop = (e: React.DragEvent, dropIndex: number) => {
    e.preventDefault();
    if (draggedIndex === null) return;
    const newFields = [...value];
    const [movedItem] = newFields.splice(draggedIndex, 1);
    newFields.splice(dropIndex, 0, movedItem);
    onChange(newFields);
    setDraggedIndex(null);
    setDragOverIndex(null);
  };

  return (
    <div className="space-y-4">
       <div className="space-y-2">
            {value.map((field, index) => {
                const TypeIcon = TYPE_CONFIG[field.type]?.icon || Type;
                const typeColor = TYPE_CONFIG[field.type]?.color || 'text-muted-foreground';

                return (
                    <div 
                        key={index}
                        draggable
                        onDragStart={(e) => handleDragStart(e, index)}
                        onDragOver={(e) => { e.preventDefault(); setDragOverIndex(index); }}
                        onDrop={(e) => handleDrop(e, index)}
                        className={`
                            group relative flex items-center gap-3 p-2.5 rounded-lg border transition-all bg-card
                            ${draggedIndex === index ? 'opacity-50 border-dashed' : 'hover:border-primary/50 hover:shadow-sm'}
                            ${dragOverIndex === index ? 'border-primary border-t-2' : 'border-border'}
                        `}
                    >
                        {/* Single Line Layout: Drag | Icon | Name | Badges | Type (Right) | Actions */}
                        
                        <div className="cursor-grab active:cursor-grabbing p-1.5 text-muted-foreground/30 hover:text-foreground rounded hover:bg-secondary">
                            <GripVertical className="h-4 w-4" />
                        </div>

                        <div className={`flex items-center justify-center h-8 w-8 rounded-md bg-secondary/50 ${typeColor}`}>
                            <TypeIcon className="h-4 w-4" />
                        </div>

                        <div className="flex items-center gap-3 min-w-0 flex-1">
                            <span className="font-semibold text-sm truncate">{field.name}</span>
                            <div className="flex gap-1">
                                {field.required && <Badge variant="destructive" className="text-[10px] h-5 px-1.5 rounded-sm font-mono">REQ</Badge>}
                                {field.unique && <Badge variant="secondary" className="text-[10px] h-5 px-1.5 rounded-sm font-mono">UNQ</Badge>}
                                {field.default && <Badge variant="outline" className="text-[10px] h-5 px-1.5 rounded-sm font-mono text-muted-foreground hidden sm:inline-flex">Def: {String(field.default)}</Badge>}
                            </div>
                        </div>

                        <div className="hidden sm:flex items-center gap-4 mr-4">
                             <div className="text-xs text-muted-foreground font-medium uppercase tracking-wider flex items-center gap-1">
                                {field.type}
                                {field.type === 'relation' && <span className="normal-case opacity-70">→ {field.relationTo}</span>}
                             </div>
                             {(field.pattern || field.min !== undefined) && (
                                <div title="Validation active">
                                    <ShieldCheck className="h-3.5 w-3.5 text-emerald-500/70" />
                                </div>
                             )}
                        </div>

                        <div className="flex items-center gap-1 border-l pl-2 ml-2 border-border/50">
                            <Button 
                                size="icon" 
                                variant="ghost" 
                                className="h-8 w-8 text-muted-foreground hover:text-primary"
                                onClick={() => setEditingIdx(index)}
                                title="Edit Field"
                            >
                                <Settings className="h-4 w-4" />
                            </Button>
                            <Button 
                                size="icon" 
                                variant="ghost" 
                                className="h-8 w-8 text-muted-foreground hover:text-destructive"
                                onClick={(e) => removeField(index, e)}
                                title="Delete Field"
                            >
                                <Trash2 className="h-4 w-4" />
                            </Button>
                        </div>
                    </div>
                );
            })}

            <Button 
                variant="outline" 
                className="w-full border-dashed py-8 text-muted-foreground hover:text-primary hover:border-primary hover:bg-primary/5 group transition-all"
                onClick={() => setIsCreating(true)}
            >
                <div className="flex flex-col items-center gap-1">
                    <div className="rounded-full bg-secondary p-2 group-hover:bg-primary/10 group-hover:text-primary transition-colors">
                        <Plus className="h-4 w-4" />
                    </div>
                    <span className="text-xs font-medium">Add New Field</span>
                </div>
            </Button>
       </div>

       {/* Editor Dialog */}
       {(isCreating || editingIdx !== null) && (
            <FieldEditorDialog 
                isOpen={true}
                isNew={isCreating}
                field={isCreating ? { name: '', type: 'text', required: false } : value[editingIdx!]}
                onSave={handleSaveField}
                onCancel={() => { setIsCreating(false); setEditingIdx(null); }}
                zIndex={zIndex + 10}
            />
       )}
    </div>
  );
};
