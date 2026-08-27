import React, { useState, useEffect, Suspense } from 'react';
import {
  Plus,
  Trash2,
  Settings,
  GripVertical,
  Check,
  X,
  Search,
  ShieldCheck,
  ChevronDown,
  Loader2,
  History,
  Fingerprint,
  Database,
  Zap,
} from 'lucide-react';
import { Button, Input, Switch, Label, Badge, Textarea } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { SchemaField, Collection } from '../../../types';
import { collectionsService } from '../services/collectionsService';
import { FIELD_TYPES_CONFIG } from '../../../config/field-types.config';

// Lazy load CollectionForm to avoid circular dependency
const CollectionForm = React.lazy(() =>
  import('./CollectionCreator').then((module) => ({ default: module.CollectionForm }))
);

interface SchemaEditorProps {
  value: SchemaField[];
  onChange: (fields: SchemaField[]) => void;
  zIndex?: number;
}

// Helper: Generate Hex ID for Uniqueness Indexing (matches Rust backend logic)
const generateHexId = () => Math.floor(Math.random() * 0xffffffff).toString(16);

// --- Components ---

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

    // Regex Pattern
    if (field.pattern) {
      try {
        const regex = new RegExp(field.pattern);
        if (!regex.test(testValue)) {
          isValid = false;
          errMsg = 'Pattern mismatch';
        }
      } catch (e) {
        isValid = false;
        errMsg = 'Invalid Regex';
      }
    }

    // Length
    if (field.minLength && testValue.length < field.minLength) {
      isValid = false;
      errMsg = `Too short (min ${field.minLength})`;
    }
    if (field.maxLength && testValue.length > field.maxLength) {
      isValid = false;
      errMsg = `Too long (max ${field.maxLength})`;
    }

    // Number Range
    if (field.type === 'number') {
      const num = Number(testValue);
      if (isNaN(num)) {
        isValid = false;
        errMsg = 'Not a number';
      } else {
        if (field.min !== null && field.min !== undefined && num < field.min) {
          isValid = false;
          errMsg = ` < Min (${field.min})`;
        }
        if (field.max !== null && field.max !== undefined && num > field.max) {
          isValid = false;
          errMsg = ` > Max (${field.max})`;
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
          placeholder="Test value..."
          className={`h-8 text-sm ${status === 'valid' ? 'border-emerald-500 focus-visible:ring-emerald-500' : status === 'invalid' ? 'border-destructive focus-visible:ring-destructive' : ''}`}
        />
        {status === 'valid' && (
          <div className="flex items-center justify-center h-8 w-8 rounded bg-emerald-500/10 text-emerald-500 shrink-0">
            <Check className="h-4 w-4" />
          </div>
        )}
        {status === 'invalid' && (
          <div
            className="flex items-center justify-center h-8 w-8 rounded bg-destructive/10 text-destructive shrink-0"
            title={error}
          >
            <X className="h-4 w-4" />
          </div>
        )}
      </div>
      {status === 'invalid' && <p className="text-[10px] text-destructive font-medium">{error}</p>}
    </div>
  );
};

const FieldEditorDialog = ({
  field,
  onSave,
  onCancel,
  isOpen,
  isNew,
  zIndex,
}: {
  field: SchemaField;
  onSave: (f: SchemaField) => void;
  onCancel: () => void;
  isOpen: boolean;
  isNew: boolean;
  zIndex: number;
}) => {
  const [data, setData] = useState<SchemaField>(field);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [selectOptions, setSelectOptions] = useState(field.options?.join('\n') || '');

  const [isColPickerOpen, setIsColPickerOpen] = useState(false);
  const [isCollectionCreatorOpen, setIsCollectionCreatorOpen] = useState(false);

  useEffect(() => {
    setData(field);
    setSelectOptions(field.options?.join('\n') || '');
  }, [field]);

  useEffect(() => {
    if (isOpen) {
      collectionsService.list().then(setCollections);
    }
  }, [isOpen]);

  const handleSave = () => {
    const updated = { ...data };
    if (data.type === 'select') {
      updated.options = selectOptions
        .split('\n')
        .map((s) => s.trim())
        .filter((s) => s);
    }

    // Clean up
    if (updated.min === ('' as any)) updated.min = null;
    if (updated.max === ('' as any)) updated.max = null;
    if (updated.minLength === ('' as any)) updated.minLength = null;
    if (updated.maxLength === ('' as any)) updated.maxLength = null;
    if (updated.dimension === ('' as any)) updated.dimension = null;
    if (!updated.vectorize) updated.dimension = null;

    onSave(updated);
  };

  const handleCreateCollection = async (collectionData: any) => {
    try {
      const newCol = await collectionsService.create(collectionData);
      setCollections((prev) => [...prev, newCol]);
      setData((prev) => ({ ...prev, relationTo: newCol.id }));
      setIsCollectionCreatorOpen(false);
      setIsColPickerOpen(false);
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <Dialog
      isOpen={isOpen}
      onClose={onCancel}
      title={isNew ? 'Add New Field' : 'Edit Field'}
      size="lg"
      zIndex={zIndex}
    >
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
                {!isNew && data.originalName && data.name !== data.originalName && (
                  <div className="flex items-center gap-1.5 text-[10px] text-amber-500 bg-amber-500/10 px-2 py-1 rounded">
                    <History className="h-3 w-3" />
                    <span>
                      Renaming from <b>{data.originalName}</b>
                    </span>
                  </div>
                )}
                <p className="text-[10px] text-muted-foreground">
                  Use lowercase, no spaces (a-z, 0-9, _)
                </p>
              </div>

              <div className="space-y-2">
                <Label>Data Type</Label>
                <div className="grid grid-cols-2 gap-2 max-h-[300px] overflow-y-auto pr-1 custom-scrollbar">
                  {Object.entries(FIELD_TYPES_CONFIG).map(([key, config]) => (
                    // When changing field type, preserve user flags instead of forcing them off:
                    <button
                      key={key}
                      type="button"
                      onClick={() =>
                        setData({
                          ...data,
                          type: key as any,
                          ose_indexed: data.ose_indexed || false,
                          sql_indexed: data.sql_indexed || false,
                          vectorize: data.vectorize || false,
                          dimension: data.vectorize ? data.dimension : null,
                        })
                      }
                      className={`flex flex-col items-start p-2.5 rounded-lg border transition-all text-left ${data.type === key ? 'border-primary bg-primary/10 ring-1 ring-primary' : 'border-border hover:bg-secondary/50 hover:border-primary/30'}`}
                    >
                      <div className="flex items-center gap-2 mb-1">
                        <config.icon
                          className={`h-4 w-4 ${data.type === key ? 'text-primary' : config.color}`}
                        />
                        <span
                          className={`text-xs font-semibold ${data.type === key ? 'text-foreground' : 'text-muted-foreground'}`}
                        >
                          {config.label}
                        </span>
                      </div>
                      <span className="text-[10px] text-muted-foreground leading-tight">
                        {config.description}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            </div>

            {['string', 'text', 'email', 'url', 'number'].includes(data.type) && (
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
                <Badge variant="outline" className="capitalize">
                  {FIELD_TYPES_CONFIG[data.type]?.label || data.type}
                </Badge>
              </div>

              {/* Common Toggles */}
              {/* In Settings Toggle Grid (Make tools available for all types) */}
              <div className="grid grid-cols-2 gap-4">
                <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                  <Label
                    className="text-xs cursor-pointer"
                    onClick={() => setData({ ...data, required: !data.required })}
                  >
                    Required
                  </Label>
                  <Switch
                    checked={data.required}
                    onCheckedChange={(c) => setData({ ...data, required: c })}
                  />
                </div>

                <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                  <Label
                    className="text-xs cursor-pointer"
                    onClick={() => setData({ ...data, unique: !data.unique })}
                  >
                    Unique
                  </Label>
                  <Switch
                    checked={data.unique}
                    onCheckedChange={(c) => setData({ ...data, unique: c })}
                  />
                </div>

                {/* SQL Index - Available for all types */}
                <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                  <div className="space-y-0.5">
                    <Label
                      className="text-xs cursor-pointer flex items-center gap-1"
                      onClick={() => setData({ ...data, sql_indexed: !data.sql_indexed })}
                    >
                      <Database className="h-3 w-3 text-muted-foreground" /> SQL Index
                    </Label>
                    <p className="text-[9px] text-muted-foreground">Faster filter/sort</p>
                  </div>
                  <Switch
                    checked={data.sql_indexed || false}
                    onCheckedChange={(c) => setData({ ...data, sql_indexed: c })}
                  />
                </div>

                {/* OSE Search Index - Available for all types */}
                <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5">
                  <div className="space-y-0.5">
                    <Label
                      className="text-xs cursor-pointer flex items-center gap-1"
                      onClick={() => setData({ ...data, ose_indexed: !data.ose_indexed })}
                    >
                      <Search className="h-3 w-3 text-muted-foreground" /> OSE Index
                    </Label>
                    <p className="text-[9px] text-muted-foreground">Full-text / Fuzzy</p>
                  </div>
                  <Switch
                    checked={data.ose_indexed || false}
                    onCheckedChange={(c) => setData({ ...data, ose_indexed: c })}
                  />
                </div>

                {/* Vectorize Toggle - Available for all types */}
                <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5 col-span-2">
                  <div className="space-y-0.5">
                    <Label
                      className="text-xs cursor-pointer flex items-center gap-1"
                      onClick={() => setData({ ...data, vectorize: !data.vectorize })}
                    >
                      <Zap className="h-3 w-3 text-purple-400" /> Vectorize (AI)
                    </Label>
                    <p className="text-[10px] text-muted-foreground">
                      Auto-generate embeddings for semantic search.
                    </p>
                  </div>
                  <Switch
                    checked={data.vectorize || false}
                    onCheckedChange={(c) => setData({ ...data, vectorize: c })}
                  />
                </div>

                {/* Auto Inject */}
                {['owner', 'date'].includes(data.type) && (
                  <div className="flex items-center justify-between p-2 rounded border border-border bg-secondary/5 col-span-2">
                    <div className="space-y-0.5">
                      <Label
                        className="text-xs cursor-pointer"
                        onClick={() => setData({ ...data, auto: !data.auto })}
                      >
                        Auto Inject
                      </Label>
                      <p className="text-[10px] text-muted-foreground">
                        {data.type === 'owner'
                          ? 'Set current User ID on create'
                          : 'Set current Timestamp on create'}
                      </p>
                    </div>
                    <Switch
                      checked={data.auto !== false}
                      onCheckedChange={(c) => setData({ ...data, auto: c })}
                    />
                  </div>
                )}
              </div>

              {/* System Properties */}
              <div className="space-y-2 bg-secondary/10 p-3 rounded-md border border-border/50">
                <div className="flex items-center justify-between">
                  <Label className="text-xs flex items-center gap-1 text-muted-foreground">
                    <Fingerprint className="h-3 w-3" /> System Index ID (UID)
                  </Label>
                  <code className="text-[10px] font-mono bg-background px-1.5 py-0.5 rounded text-foreground">
                    {data.uid || 'Pending...'}
                  </code>
                </div>
              </div>

              {/* Default Value */}
              {!['file', 'blob', 'relation', 'vector', 'owner'].includes(data.type) && (
                <div className="space-y-2">
                  <Label>Default Value</Label>
                  <Input
                    value={data.default || ''}
                    onChange={(e: any) => setData({ ...data, default: e.target.value })}
                    placeholder="Optional default..."
                    className="h-8 text-sm"
                  />
                </div>
              )}

              {/* --- TYPE SPECIFIC CONFIGS --- */}
              <div className="space-y-4 animate-in fade-in duration-300">
                {/* SELECT */}
                {data.type === 'select' && (
                  <div className="space-y-2">
                    <Label>Options (one per line)</Label>
                    <Textarea
                      value={selectOptions}
                      onChange={(e: any) => setSelectOptions(e.target.value)}
                      placeholder="Draft&#10;Published&#10;Archived"
                      className="min-h-[100px] font-mono text-xs"
                    />
                  </div>
                )}

                {/* NUMBER */}
                {data.type === 'number' && (
                  <div className="grid grid-cols-2 gap-3">
                    <div className="space-y-1">
                      <Label className="text-xs">Min Value</Label>
                      <Input
                        type="number"
                        value={data.min ?? ''}
                        onChange={(e: any) =>
                          setData({ ...data, min: e.target.value ? Number(e.target.value) : null })
                        }
                        className="h-8"
                      />
                    </div>
                    <div className="space-y-1">
                      <Label className="text-xs">Max Value</Label>
                      <Input
                        type="number"
                        value={data.max ?? ''}
                        onChange={(e: any) =>
                          setData({ ...data, max: e.target.value ? Number(e.target.value) : null })
                        }
                        className="h-8"
                      />
                    </div>
                  </div>
                )}

                {/* STRINGS */}
                {['string', 'text', 'email', 'url', 'blob'].includes(data.type) && (
                  <div className="space-y-3">
                    <div className="space-y-1">
                      <Label className="text-xs">Regex Pattern</Label>
                      <Input
                        value={data.pattern || ''}
                        onChange={(e: any) => setData({ ...data, pattern: e.target.value })}
                        className="h-8 font-mono text-xs"
                        placeholder="e.g. ^[A-Z]+$"
                      />
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                      <div className="space-y-1">
                        <Label className="text-xs">Min Length</Label>
                        <Input
                          type="number"
                          value={data.minLength ?? ''}
                          onChange={(e: any) =>
                            setData({
                              ...data,
                              minLength: e.target.value ? Number(e.target.value) : null,
                            })
                          }
                          className="h-8"
                        />
                      </div>
                      <div className="space-y-1">
                        <Label className="text-xs">Max Length</Label>
                        <Input
                          type="number"
                          value={data.maxLength ?? ''}
                          onChange={(e: any) =>
                            setData({
                              ...data,
                              maxLength: e.target.value ? Number(e.target.value) : null,
                            })
                          }
                          className="h-8"
                        />
                      </div>
                    </div>
                  </div>
                )}

                {/* FILE / BLOB */}
                {['file', 'blob'].includes(data.type) && (
                  <div className="space-y-3">
                    <div className="space-y-1">
                      <Label className="text-xs">Max Size (bytes)</Label>
                      <Input
                        type="number"
                        value={data.maxSize ?? ''}
                        onChange={(e: any) =>
                          setData({
                            ...data,
                            maxSize: e.target.value ? Number(e.target.value) : null,
                          })
                        }
                        className="h-8"
                        placeholder="5242880 (5MB)"
                      />
                    </div>
                    <div className="space-y-1">
                      <Label className="text-xs">Allowed MIME Types</Label>
                      <Input
                        value={data.mimeTypes?.join(', ') || ''}
                        onChange={(e: any) =>
                          setData({
                            ...data,
                            mimeTypes: e.target.value.split(',').map((t: string) => t.trim()),
                          })
                        }
                        className="h-8"
                        placeholder="image/png, application/pdf"
                      />
                    </div>
                  </div>
                )}

                {/* VECTOR DIMENSION */}
                {['string', 'text', 'file'].includes(data.type) && data.vectorize && (
                  <div className="space-y-2 animate-in fade-in duration-300">
                    <Label className="text-xs">Vector Dimensions</Label>
                    <Input
                      type="number"
                      value={data.dimension ?? ''}
                      onChange={(e: any) =>
                        setData({
                          ...data,
                          dimension: e.target.value ? Number(e.target.value) : null,
                        })
                      }
                      className="h-8"
                      placeholder="e.g. 512 (Gemma), 768 (CLIP)"
                    />
                    <p className="text-[10px] text-muted-foreground">
                      Required for similarity search.
                    </p>
                  </div>
                )}

                {/* RELATION / OWNER */}
                {(data.type === 'relation' || data.type === 'owner') && (
                  <div className="space-y-4 animate-in fade-in duration-300">
                    <div className="space-y-2">
                      <Label required>
                        {data.type === 'owner' ? 'User Collection' : 'Related Collection'}
                      </Label>
                      <div className="relative">
                        <Button
                          type="button"
                          variant="outline"
                          className="w-full justify-between text-left font-normal px-3 h-9"
                          onClick={() => setIsColPickerOpen(true)}
                        >
                          {data.relationTo ? (
                            <span className="flex items-center gap-2">
                              <div className="h-2 w-2 rounded-full bg-emerald-500"></div>
                              <span className="font-semibold text-foreground">
                                {collections.find(
                                  (c) => c.id === data.relationTo || c.name === data.relationTo
                                )?.name || data.relationTo}
                              </span>
                            </span>
                          ) : (
                            <span className="text-muted-foreground">Select Collection...</span>
                          )}
                          <ChevronDown className="h-4 w-4 opacity-50" />
                        </Button>

                        {isColPickerOpen && (
                          <div className="absolute top-full left-0 right-0 z-50 mt-1 max-h-48 overflow-y-auto rounded-md border bg-popover p-1 shadow-md">
                            {collections.map((c) => (
                              <button
                                key={c.id}
                                className="w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-accent flex items-center justify-between"
                                onClick={() => {
                                  setData({ ...data, relationTo: c.id });
                                  setIsColPickerOpen(false);
                                }}
                              >
                                <span>{c.name}</span>
                                <Badge variant="secondary" className="text-[10px]">
                                  {c.type}
                                </Badge>
                              </button>
                            ))}
                            <button
                              className="w-full rounded-sm px-2 py-1.5 text-left text-sm text-primary hover:bg-accent font-medium flex items-center gap-1 border-t mt-1 pt-2"
                              onClick={() => {
                                setIsColPickerOpen(false);
                                setIsCollectionCreatorOpen(true);
                              }}
                            >
                              <Plus className="h-3 w-3" /> Create New
                            </button>
                          </div>
                        )}
                      </div>
                    </div>

                    {data.type === 'relation' && (
                      <div className="flex items-center justify-between p-3 rounded-lg border border-border bg-secondary/5">
                        <div className="space-y-0.5">
                          <Label
                            className="text-xs cursor-pointer flex items-center gap-1"
                            onClick={() =>
                              setData({
                                ...data,
                                cascade_on_target_delete: !data.cascade_on_target_delete,
                              })
                            }
                          >
                            <Trash2 className="h-3 w-3 text-muted-foreground" /> Cascade Delete
                          </Label>
                          <p className="text-[10px] text-muted-foreground leading-relaxed max-w-[250px]">
                            When the target record is deleted, delete this record automatically.
                          </p>
                        </div>
                        <Switch
                          checked={data.cascade_on_target_delete || false}
                          onCheckedChange={(c) => setData({ ...data, cascade_on_target_delete: c })}
                        />
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        <div className="flex justify-end gap-3 pt-6 border-t border-border mt-4">
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!data.name}>
            <Check className="mr-2 h-4 w-4" /> {isNew ? 'Add Field' : 'Save Changes'}
          </Button>
        </div>
      </div>
    </Dialog>
  );
};

// --- Main SchemaEditor Component ---

export const SchemaEditor = ({ value, onChange, zIndex = 60 }: SchemaEditorProps) => {
  const [editingIdx, setEditingIdx] = useState<number | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);

  const handleSaveField = (field: SchemaField) => {
    const newFields = [...value];
    if (isCreating) {
      newFields.push({ ...field, originalName: field.name, uid: field.uid || generateHexId() });
    } else if (editingIdx !== null) {
      const prev = newFields[editingIdx];
      newFields[editingIdx] = { ...field, originalName: prev.originalName || prev.name };
    }
    const reorderedFields = newFields.map((f, idx) => ({ ...f, position: idx }));
    onChange(reorderedFields);
    setEditingIdx(null);
    setIsCreating(false);
  };

  const removeField = (index: number, e: React.MouseEvent) => {
    e.stopPropagation();
    const newFields = [...value];
    newFields.splice(index, 1);
    const reorderedFields = newFields.map((f, idx) => ({ ...f, position: idx }));
    onChange(reorderedFields);
  };

  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = 'move';
  };
  const handleDrop = (e: React.DragEvent, dropIndex: number) => {
    e.preventDefault();
    if (draggedIndex === null) return;
    const newFields = [...value];
    const [movedItem] = newFields.splice(draggedIndex, 1);
    newFields.splice(dropIndex, 0, movedItem);
    const reorderedFields = newFields.map((f, idx) => ({ ...f, position: idx }));
    onChange(reorderedFields);
    setDraggedIndex(null);
    setDragOverIndex(null);
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        {value.map((field, index) => {
          const config = FIELD_TYPES_CONFIG[field.type] || FIELD_TYPES_CONFIG.string;
          const TypeIcon = config.icon;
          const typeColor = config.color;

          return (
            <div
              key={index}
              draggable
              onDragStart={(e) => handleDragStart(e, index)}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOverIndex(index);
              }}
              onDrop={(e) => handleDrop(e, index)}
              className={`group relative flex items-center gap-3 p-2.5 rounded-lg border transition-all bg-card ${draggedIndex === index ? 'opacity-50 border-dashed' : 'hover:border-primary/50 hover:shadow-sm'} ${dragOverIndex === index ? 'border-primary border-t-2' : 'border-border'}`}
            >
              <div className="cursor-grab active:cursor-grabbing p-1.5 text-muted-foreground/30 hover:text-foreground rounded hover:bg-secondary">
                <GripVertical className="h-4 w-4" />
              </div>
              <div
                className={`flex items-center justify-center h-8 w-8 rounded-md bg-secondary/50 ${typeColor}`}
              >
                <TypeIcon className="h-4 w-4" />
              </div>

              <div className="flex items-center gap-3 min-w-0 flex-1">
                <span className="font-semibold text-sm truncate">{field.name}</span>
                <div className="flex gap-1">
                  {field.required && (
                    <Badge
                      variant="destructive"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono"
                    >
                      REQ
                    </Badge>
                  )}
                  {field.unique && (
                    <Badge
                      variant="secondary"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono"
                    >
                      UNQ
                    </Badge>
                  )}
                  {field.sql_indexed && (
                    <Badge
                      variant="secondary"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono flex items-center gap-0.5"
                    >
                      <Database className="h-2 w-2" /> SQL
                    </Badge>
                  )}
                  {field.ose_indexed && (
                    <Badge
                      variant="secondary"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono flex items-center gap-0.5"
                    >
                      <Search className="h-2 w-2" /> OSE
                    </Badge>
                  )}
                  {field.vectorize && (
                    <Badge
                      variant="secondary"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono bg-indigo-500/10 text-indigo-400"
                    >
                      VEC
                    </Badge>
                  )}
                  {field.auto && (
                    <Badge
                      variant="secondary"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono bg-emerald-500/10 text-emerald-400"
                    >
                      AUTO
                    </Badge>
                  )}
                  {field.originalName && field.name !== field.originalName && (
                    <Badge
                      variant="warning"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono flex items-center gap-1"
                      title={`Renamed from ${field.originalName}`}
                    >
                      <History className="h-3 w-3" /> RENAMED
                    </Badge>
                  )}
                  {field.cascade_on_target_delete && (
                    <Badge
                      variant="secondary"
                      className="text-[10px] h-5 px-1.5 rounded-sm font-mono flex items-center gap-0.5 bg-red-500/10 text-red-400 border-red-500/20"
                      title="Cascades on Target Delete"
                    >
                      <Trash2 className="h-2 w-2" /> CASCADE
                    </Badge>
                  )}
                </div>
              </div>

              <div className="hidden sm:flex items-center gap-4 mr-4">
                <div className="text-xs text-muted-foreground font-medium uppercase tracking-wider flex items-center gap-1">
                  {field.type}
                  {(field.type === 'relation' || field.type === 'owner') && (
                    <span className="normal-case opacity-70">→ {field.relationTo}</span>
                  )}
                  {['string', 'text', 'file'].includes(field.type) && field.vectorize && (
                    <span className="normal-case opacity-70">[{field.dimension || '?'}]</span>
                  )}
                </div>
                {field.uid && (
                  <span className="text-[9px] font-mono text-muted-foreground/40 select-all">
                    #{field.uid.substring(0, 4)}
                  </span>
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

      {(isCreating || editingIdx !== null) && (
        <FieldEditorDialog
          isOpen={true}
          isNew={isCreating}
          field={
            isCreating
              ? {
                  name: '',
                  type: 'string',
                  required: false,
                  ose_indexed: false,
                  sql_indexed: false,
                  auto: false,
                  uid: generateHexId(),
                  position: value.length,
                }
              : value[editingIdx!]
          }
          onSave={handleSaveField}
          onCancel={() => {
            setIsCreating(false);
            setEditingIdx(null);
          }}
          zIndex={zIndex + 10}
        />
      )}
    </div>
  );
};
