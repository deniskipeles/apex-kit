import React, { useState } from 'react';
import { Plus, Trash2, Settings, GripVertical, X } from 'lucide-react';
import { Button, Input, Select, Switch, Label, Badge, Separator } from './ui/Elements';
import { Dialog } from './ui/Dialog';
import { SchemaField } from '../types';

interface SchemaBuilderProps {
  value: SchemaField[];
  onChange: (fields: SchemaField[]) => void;
}

const FIELD_TYPES = [
  { value: 'text', label: 'Text' },
  { value: 'number', label: 'Number' },
  { value: 'bool', label: 'Boolean' },
  { value: 'email', label: 'Email' },
  { value: 'url', label: 'URL' },
  { value: 'date', label: 'Date' },
  { value: 'select', label: 'Select' },
  { value: 'json', label: 'JSON' },
  { value: 'file', label: 'File' },
  { value: 'relation', label: 'Relation' },
];

export const SchemaBuilder = ({ value, onChange }: SchemaBuilderProps) => {
  const [editingIndex, setEditingIndex] = useState<number | null>(null);

  // Drag and Drop State
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);

  const addField = () => {
    const newField: SchemaField = {
      name: `field_${value.length + 1}`,
      type: 'text',
      required: false,
    };
    onChange([...value, newField]);
  };

  const removeField = (index: number) => {
    const newFields = [...value];
    newFields.splice(index, 1);
    onChange(newFields);
  };

  const updateField = (index: number, updates: Partial<SchemaField>) => {
    const newFields = [...value];
    newFields[index] = { ...newFields[index], ...updates };
    onChange(newFields);
  };

  // --- DnD Handlers ---

  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', index.toString());
  };

  const handleDragEnd = (e: React.DragEvent) => {
    const target = e.target as HTMLElement;
    target.style.opacity = '';
    setDraggedIndex(null);
    setDragOverIndex(null);
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

  const activeField = editingIndex !== null ? value[editingIndex] : null;

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-border bg-card overflow-hidden">
        {/* Header */}
        <div className="grid grid-cols-12 gap-4 bg-secondary/30 px-4 py-3 text-xs font-medium text-muted-foreground border-b border-border">
          <div className="col-span-1"></div>
          <div className="col-span-4">Field Name</div>
          <div className="col-span-3">Type</div>
          <div className="col-span-2 text-center">Required</div>
          <div className="col-span-2 text-right">Actions</div>
        </div>

        {/* List */}
        <div className="divide-y divide-border">
          {value.map((field, index) => (
            <div
              key={index}
              draggable
              onDragStart={(e) => handleDragStart(e, index)}
              onDragEnd={handleDragEnd}
              onDragOver={(e) => {
                e.preventDefault();
                setDragOverIndex(index);
              }}
              onDrop={(e) => handleDrop(e, index)}
              className={`grid grid-cols-12 gap-4 p-3 items-center transition-colors group relative 
                ${draggedIndex === index ? 'bg-secondary/20' : 'hover:bg-secondary/5'}
                ${dragOverIndex === index ? 'border-t-2 border-t-primary bg-primary/5' : ''}
              `}
            >
              <div className="col-span-1 flex justify-center cursor-grab active:cursor-grabbing touch-none">
                <GripVertical className="h-4 w-4 text-muted-foreground/50 hover:text-primary" />
              </div>

              <div className="col-span-4">
                <Input
                  value={field.name}
                  onChange={(e: any) => updateField(index, { name: e.target.value })}
                  className="h-8 font-mono text-sm"
                  placeholder="field_name"
                />
              </div>

              <div className="col-span-3">
                <Select
                  value={field.type}
                  onChange={(e: any) => updateField(index, { type: e.target.value as any })}
                  className="h-8"
                >
                  {FIELD_TYPES.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </Select>
              </div>

              <div className="col-span-2 flex justify-center">
                <Switch
                  checked={field.required}
                  onCheckedChange={(checked: boolean) => updateField(index, { required: checked })}
                  className="scale-75"
                />
              </div>

              <div className="col-span-2 flex justify-end gap-1">
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-8 w-8 text-muted-foreground hover:text-primary"
                  onClick={() => setEditingIndex(index)}
                >
                  <Settings className="h-4 w-4" />
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="h-8 w-8 text-muted-foreground hover:text-destructive"
                  onClick={() => removeField(index)}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            </div>
          ))}
        </div>

        <div className="bg-secondary/10 p-3 border-t border-border">
          <Button
            onClick={addField}
            variant="outline"
            size="sm"
            className="w-full border-dashed hover:border-primary hover:bg-primary/5 hover:text-primary transition-all"
          >
            <Plus className="mr-2 h-4 w-4" /> Add New Field
          </Button>
        </div>
      </div>

      {activeField && editingIndex !== null && (
        <Dialog
          isOpen={true}
          onClose={() => setEditingIndex(null)}
          title={`Configure "${activeField.name}"`}
          description={`Adjust validation and advanced settings for this ${activeField.type} field.`}
          size="lg"
        >
          <div className="space-y-6 py-2">
            <div className="grid grid-cols-2 gap-6">
              <div className="space-y-4">
                <h4 className="text-sm font-medium text-foreground flex items-center gap-2">
                  General Options
                </h4>
                <Separator />

                <div className="flex items-center justify-between rounded-md border border-border p-3 bg-secondary/10">
                  <div className="space-y-0.5">
                    <Label className="text-base">Required</Label>
                    <p className="text-xs text-muted-foreground">Value cannot be empty</p>
                  </div>
                  <Switch
                    checked={activeField.required}
                    onCheckedChange={(c: boolean) => updateField(editingIndex, { required: c })}
                  />
                </div>

                <div className="flex items-center justify-between rounded-md border border-border p-3 bg-secondary/10">
                  <div className="space-y-0.5">
                    <Label className="text-base">Unique</Label>
                    <p className="text-xs text-muted-foreground">No duplicate values allowed</p>
                  </div>
                  <Switch
                    checked={activeField.unique || false}
                    onCheckedChange={(c: boolean) => updateField(editingIndex, { unique: c })}
                  />
                </div>
              </div>

              <div className="space-y-4">
                <h4 className="text-sm font-medium text-foreground flex items-center gap-2">
                  Type Specific
                  <Badge variant="secondary" className="text-[10px] uppercase">
                    {activeField.type}
                  </Badge>
                </h4>
                <Separator />

                {activeField.type === 'number' && (
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                      <Label>Min Value</Label>
                      <Input
                        type="number"
                        value={activeField.min ?? ''}
                        onChange={(e: any) =>
                          updateField(editingIndex, {
                            min: e.target.value ? Number(e.target.value) : null,
                          })
                        }
                      />
                    </div>
                    <div className="space-y-2">
                      <Label>Max Value</Label>
                      <Input
                        type="number"
                        value={activeField.max ?? ''}
                        onChange={(e: any) =>
                          updateField(editingIndex, {
                            max: e.target.value ? Number(e.target.value) : null,
                          })
                        }
                      />
                    </div>
                  </div>
                )}
              </div>
            </div>

            <div className="flex justify-end pt-4 border-t border-border">
              <Button onClick={() => setEditingIndex(null)}>Done</Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
};
