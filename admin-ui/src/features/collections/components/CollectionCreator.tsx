import React, { useState } from 'react';
import { Save, Shield, AlertCircle, Fingerprint, Plus, Trash2, FileJson, Type } from 'lucide-react';
import {
  Button,
  Input,
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Label,
  Badge,
  Checkbox,
} from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { SchemaEditor } from './SchemaEditor';
import { SchemaField, Collection } from '../../../types';
import { JSONEditor } from '../../../components/form/JsonEditor';

interface CollectionFormProps {
  onSave: (data: any) => void;
  onCancel: () => void;
  isEmbedded?: boolean;
  zIndex?: number;
  initialValues?: Collection | null;
}

// Helper to check if a string is valid JSON
const isJsonString = (str: string) => {
  try {
    const parsed = JSON.parse(str);
    return typeof parsed === 'object' && parsed !== null;
  } catch (e) {
    return false;
  }
};

// --- Helper Component for the Index Modal ---
const CompositeIndexModal = ({
  isOpen,
  onClose,
  fields,
  onAdd,
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
          Select the fields that should form a unique combination. Rows with duplicate values across{' '}
          <em>all</em> these fields will be rejected.
        </p>
        <div className="max-h-[300px] overflow-y-auto border border-border rounded-md p-2 space-y-1">
          {fields.map((field) => (
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
          {fields.length === 0 && (
            <div className="text-sm text-muted-foreground p-2">No fields defined yet.</div>
          )}
        </div>
        <div className="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={selected.size < 2}>
            Create Index
          </Button>
        </div>
      </div>
    </Dialog>
  );
};

export const CollectionForm = ({
  onSave,
  onCancel,
  isEmbedded = false,
  zIndex = 0,
  initialValues,
}: CollectionFormProps) => {
  const [name, setName] = useState(initialValues?.name || '');
  const [schema, setSchema] = useState<SchemaField[]>(
    initialValues?.schema || [
      {
        name: 'title',
        type: 'string',
        required: true,
        sql_indexed: true,
        uid: Math.floor(Math.random() * 0xffffffff).toString(16),
        position: 0,
      },
    ]
  );

  // State for Composite Indexes (Array of Arrays of strings)
  const [compositeUnique, setCompositeUnique] = useState<string[][]>(
    initialValues?.compositeUnique || []
  );
  const [isIndexModalOpen, setIsIndexModalOpen] = useState(false);

  // Rules State
  const [rules, setRules] = useState(
    initialValues?.rules || {
      read: 'public',
      create: 'admin',
      update: 'admin',
      delete: 'admin',
    }
  );

  // [NEW] Track mode per rule
  const [ruleModes, setRuleModes] = useState({
    read: isJsonString(initialValues?.rules?.read || 'public'),
    create: isJsonString(initialValues?.rules?.create || 'admin'),
    update: isJsonString(initialValues?.rules?.update || 'admin'),
    delete: isJsonString(initialValues?.rules?.delete || 'admin'),
  });

  const effectiveZIndex = zIndex > 0 ? zIndex : 70;

  const handleSave = () => {
    const identifierRegex = /^[a-zA-Z0-9_]+$/;
    if (!identifierRegex.test(name)) {
      alert('Name can only contain letters, numbers, and underscores.');
      return;
    }

    for (const field of schema) {
      if (!identifierRegex.test(field.name)) {
        alert(`Field name '${field.name}' is invalid. Use only letters, numbers, and underscores.`);
        return;
      }
    }

    // Clean up rules before save (minify JSON if applicable)
    const cleanRules = { ...rules };
    for (const key of ['read', 'create', 'update', 'delete'] as const) {
      if (ruleModes[key]) {
        try {
          const parsed = JSON.parse(cleanRules[key]);
          cleanRules[key] = JSON.stringify(parsed);
        } catch {
          alert(`Invalid JSON in ${key} rule`);
          return;
        }
      }
    }

    // Combine everything
    onSave({
      name,
      schema,
      rules: cleanRules,
      compositeUnique,
    });
  };

  // [NEW] Helper to render the unified policy input
  const renderRuleInput = (
    key: 'read' | 'create' | 'update' | 'delete',
    label: string,
    desc: string
  ) => {
    const isJson = ruleModes[key];
    const value = rules[key];

    const toggleMode = (toJson: boolean) => {
      if (toJson && !isJson) {
        setRules({ ...rules, [key]: '{\n  \n}' });
      } else if (!toJson && isJson) {
        setRules({ ...rules, [key]: '' });
      }
      setRuleModes({ ...ruleModes, [key]: toJson });
    };

    return (
      <div key={key} className="grid gap-1.5 group flex-1 flex flex-col min-h-0">
        <div className="flex items-center justify-between">
          <div>
            <Label className="text-xs font-semibold text-foreground group-hover:text-primary transition-colors capitalize">
              {label}
            </Label>
            <span className="text-[10px] text-muted-foreground ml-2 hidden sm:inline">{desc}</span>
          </div>
          <div className="flex items-center gap-1 bg-secondary/30 p-0.5 rounded-lg border border-border">
            <button
              onClick={() => toggleMode(false)}
              className={`px-2 py-0.5 rounded text-[10px] flex items-center gap-1 transition-all ${!isJson ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <Type className="h-3 w-3" /> Legacy
            </button>
            <button
              onClick={() => toggleMode(true)}
              className={`px-2 py-0.5 rounded text-[10px] flex items-center gap-1 transition-all ${isJson ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <FileJson className="h-3 w-3" /> JSON
            </button>
          </div>
        </div>

        {isJson ? (
          <div className="border border-input rounded-md overflow-hidden min-h-[150px] flex-1">
            <JSONEditor
              value={value}
              onChange={(val) => setRules({ ...rules, [key]: val })}
              height="100%"
            />
          </div>
        ) : (
          <div className="relative">
            <Input
              className="font-mono text-xs h-9 bg-background/50 focus:bg-background transition-colors pr-8"
              placeholder="public"
              value={value}
              onChange={(e: any) => setRules({ ...rules, [key]: e.target.value })}
            />
            <div className="absolute right-2 top-2.5 h-2 w-2 rounded-full bg-border group-focus-within:bg-primary transition-colors"></div>
          </div>
        )}
      </div>
    );
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
            <h2 className="text-3xl font-bold tracking-tight">
              {initialValues ? 'Edit Collection' : 'Create Collection'}
            </h2>
            <p className="text-muted-foreground">
              {initialValues ? 'Modify schema and access rules.' : 'Define schema and rules.'}
            </p>
          </div>
        )}
        {isEmbedded && <div></div>}
        <div className="flex gap-3">
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={handleSave}>
            <Save className="mr-2 h-4 w-4" /> Save Collection
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div className="lg:col-span-2 space-y-6">
          {/* 1. Details */}
          <Card>
            <CardHeader>
              <CardTitle>Details</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-2">
                <Label>Name</Label>
                <Input
                  value={name}
                  onChange={(e: any) => setName(e.target.value)}
                  placeholder="e.g. posts"
                  autoFocus={isEmbedded}
                />
              </div>
            </CardContent>
          </Card>

          {/* 2. Fields */}
          <Card>
            <CardHeader>
              <CardTitle>Fields</CardTitle>
            </CardHeader>
            <div className="p-4 pt-0">
              <SchemaEditor value={schema} onChange={setSchema} zIndex={effectiveZIndex} />
            </div>
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
                <p className="text-sm text-muted-foreground italic">
                  No composite unique constraints defined.
                </p>
              ) : (
                <div className="space-y-2">
                  {compositeUnique.map((group, idx) => (
                    <div
                      key={idx}
                      className="flex items-center justify-between p-2 rounded border border-border bg-card"
                    >
                      <div className="flex flex-wrap gap-1.5 items-center">
                        <span className="text-xs font-bold text-muted-foreground mr-1">UNIQUE</span>
                        <span className="text-xs text-muted-foreground">(</span>
                        {group.map((f, i) => (
                          <React.Fragment key={i}>
                            <Badge variant="secondary" className="font-mono text-[10px]">
                              {f}
                            </Badge>
                            {i < group.length - 1 && (
                              <span className="text-muted-foreground text-xs">+</span>
                            )}
                          </React.Fragment>
                        ))}
                        <span className="text-xs text-muted-foreground">)</span>
                      </div>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-7 w-7 text-muted-foreground hover:text-destructive"
                        onClick={() => removeCompositeIndex(idx)}
                      >
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
        <div className="space-y-6 flex flex-col h-full">
          <Card className="flex-1 flex flex-col">
            <CardHeader className="pb-3 shrink-0">
              <CardTitle className="flex items-center gap-2">
                <Shield className="h-4 w-4 text-primary" /> API Rules
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-5 flex-1 flex flex-col overflow-y-auto">
              <div className="rounded-md bg-primary/10 p-3 text-xs text-primary leading-relaxed shrink-0">
                Rules determine who can access your data. Leave empty to allow public access. Set to{' '}
                <code>null</code> to restrict access.
              </div>

              <div className="space-y-6 flex-1 flex flex-col">
                {renderRuleInput('read', 'Read Rule', 'List and view records')}
                {renderRuleInput('create', 'Create Rule', 'Create new records')}
                {renderRuleInput('update', 'Update Rule', 'Edit existing records')}
                {renderRuleInput('delete', 'Delete Rule', 'Remove records')}
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
