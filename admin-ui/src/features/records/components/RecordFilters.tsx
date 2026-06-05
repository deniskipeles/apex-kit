import React, { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { X, Filter, Search, Plus, Trash2, Check } from 'lucide-react';
import { Button, Input, Select, Badge } from '../../../components/ui/Elements'; // Use generic Elements export
import { Collection } from '../../../types';

interface RecordFiltersProps {
  isOpen: boolean;
  onClose: () => void;
  collection: Collection | null;
  onApplyFilters: (filters: any) => void;
}

// Map UI labels to Backend MongoDB Operators
const OPERATORS = [
  { label: 'Equals (=)', value: '$eq' },
  { label: 'Not Equals (!=)', value: '$neq' },
  { label: 'Contains (Text)', value: '$contains' },
  { label: 'Like (Wildcard %)', value: '$like' },
  { label: 'Greater Than (>)', value: '$gt' },
  { label: 'Greater/Equal (>=)', value: '$gte' },
  { label: 'Less Than (<)', value: '$lt' },
  { label: 'Less/Equal (<=)', value: '$lte' },
  { label: 'In List (comma sep)', value: '$in' },
  { label: 'Not In List', value: '$nin' },
];

export const RecordFilters = ({
  isOpen,
  onClose,
  collection,
  onApplyFilters,
}: RecordFiltersProps) => {
  // State: Map of fieldName -> { operator, value }
  const [activeFilters, setActiveFilters] = useState<Record<string, { op: string; val: string }>>(
    {}
  );

  // Reset when collection changes
  useEffect(() => {
    setActiveFilters({});
  }, [collection?.id]);

  const updateFilter = (field: string, key: 'op' | 'val', value: string) => {
    setActiveFilters((prev) => ({
      ...prev,
      [field]: {
        op: key === 'op' ? value : prev[field]?.op || '$eq',
        val: key === 'val' ? value : prev[field]?.val || '',
      },
    }));
  };

  const clearFilter = (field: string) => {
    const next = { ...activeFilters };
    delete next[field];
    setActiveFilters(next);
  };

  const handleApply = () => {
    const mongoQuery: any = {};

    Object.entries(activeFilters).forEach(([field, { op, val }]) => {
      if (val === '' || val === undefined) return;

      let processedVal: any = val;
      const fieldDef = collection?.schema.find((f) => f.name === field);

      // 1. Type Coercion
      if (fieldDef?.type === 'number') {
        processedVal = Number(val);
      } else if (fieldDef?.type === 'bool') {
        processedVal = val === 'true';
      }

      // 2. Array Handling for $in / $nin
      if (op === '$in' || op === '$nin') {
        // Split by comma, trim, and convert types if needed
        const arr = String(val)
          .split(',')
          .map((s) => s.trim());
        if (fieldDef?.type === 'number') {
          processedVal = arr.map(Number).filter((n) => !isNaN(n));
        } else {
          processedVal = arr;
        }
      }

      // 3. Construct Mongo Syntax
      if (op === '$eq') {
        // Implicit equality: { "status": "draft" }
        mongoQuery[field] = processedVal;
      } else {
        // Operator syntax: { "views": { "$gt": 100 } }
        mongoQuery[field] = { [op]: processedVal };
      }
    });

    onApplyFilters(mongoQuery);
    onClose();
  };

  if (!isOpen) return null;

  const activeCount = Object.keys(activeFilters).filter((k) => activeFilters[k].val).length;

  return createPortal(
    <div className="fixed inset-0 z-[60] flex justify-end isolate">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in"
        onClick={onClose}
      />

      {/* Panel */}
      <div className="relative w-full md:max-w-md h-full bg-background border-l border-border shadow-2xl animate-in slide-in-from-right flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b bg-secondary/5">
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-bold flex items-center gap-2">
              <Filter className="h-4 w-4" /> Filter Records
            </h2>
            {activeCount > 0 && (
              <Badge variant="primary" className="rounded-full px-2">
                {activeCount}
              </Badge>
            )}
          </div>
          <Button size="icon" variant="ghost" onClick={onClose}>
            <X className="h-5 w-5" />
          </Button>
        </div>

        {/* Filter List */}
        <div className="flex-1 overflow-y-auto p-6 space-y-4 custom-scrollbar">
          {collection?.schema.map((field) => {
            const filter = activeFilters[field.name] || { op: '$eq', val: '' };
            const isActive = !!activeFilters[field.name]?.val; // Only active if value present

            return (
              <div
                key={field.name}
                className={`p-3 rounded-lg border transition-all duration-200 ${isActive ? 'border-primary/50 bg-primary/5 shadow-sm' : 'border-border bg-card hover:border-primary/20'}`}
              >
                {/* Field Header */}
                <div className="flex items-center justify-between mb-2">
                  <label className="text-sm font-semibold flex items-center gap-2 text-foreground">
                    {field.name}
                    <span
                      className={`text-[10px] font-mono uppercase px-1.5 py-0.5 rounded border ${isActive ? 'bg-background border-primary/20 text-primary' : 'bg-secondary border-transparent text-muted-foreground'}`}
                    >
                      {field.type}
                    </span>
                  </label>
                  {isActive && (
                    <button
                      onClick={() => clearFilter(field.name)}
                      className="text-muted-foreground hover:text-destructive transition-colors p-1 rounded hover:bg-destructive/10"
                      title="Clear filter"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  )}
                </div>

                {/* Controls */}
                <div className="flex gap-2">
                  {/* Operator Selector */}
                  <div className="w-[110px] shrink-0">
                    <Select
                      className="h-9 text-xs font-medium"
                      value={filter.op}
                      onChange={(e: any) => updateFilter(field.name, 'op', e.target.value)}
                    >
                      {OPERATORS.map((op) => (
                        <option key={op.value} value={op.value}>
                          {op.label}
                        </option>
                      ))}
                    </Select>
                  </div>

                  {/* Value Input */}
                  <div className="flex-1">
                    {field.type === 'bool' ? (
                      <Select
                        className="h-9 text-xs"
                        value={filter.val}
                        onChange={(e: any) => updateFilter(field.name, 'val', e.target.value)}
                      >
                        <option value="">-- Any --</option>
                        <option value="true">True</option>
                        <option value="false">False</option>
                      </Select>
                    ) : (
                      <Input
                        className="h-9 text-xs"
                        placeholder={filter.op.includes('in') ? 'e.g. apple, banana' : 'Value...'}
                        value={filter.val}
                        onChange={(e: any) => updateFilter(field.name, 'val', e.target.value)}
                        type={
                          field.type === 'number' && !filter.op.includes('in') ? 'number' : 'text'
                        }
                      />
                    )}
                  </div>
                </div>
              </div>
            );
          })}

          {(!collection?.schema || collection.schema.length === 0) && (
            <div className="text-center text-muted-foreground text-sm py-8">
              No fields available to filter.
            </div>
          )}
        </div>

        {/* Footer Actions */}
        <div className="p-4 border-t flex gap-3 bg-background safe-bottom">
          <Button variant="outline" className="flex-1" onClick={() => setActiveFilters({})}>
            Reset All
          </Button>
          <Button className="flex-1" onClick={handleApply}>
            <Check className="mr-2 h-4 w-4" /> Apply Filters
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
