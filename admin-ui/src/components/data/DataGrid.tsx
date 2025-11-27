
import React from 'react';
import { Checkbox } from '../form/FormPrimitives';

interface Column<T> {
  field: keyof T | string;
  headerName: string;
  renderCell?: (item: T) => React.ReactNode;
  width?: string;
  align?: 'left' | 'center' | 'right';
}

interface DataGridProps<T> {
  data: T[];
  columns: Column<T>[];
  keyField: keyof T;
  selectedIds?: string[];
  onSelectAll?: () => void;
  onSelectRow?: (id: string) => void;
  onRowClick?: (item: T) => void;
  isLoading?: boolean;
}

export function DataGrid<T extends { [key: string]: any }>({
  data,
  columns,
  keyField,
  selectedIds = [],
  onSelectAll,
  onSelectRow,
  onRowClick,
  isLoading
}: DataGridProps<T>) {
  
  const allSelected = data.length > 0 && selectedIds.length === data.length;

  return (
    <div className="relative w-full h-full overflow-hidden rounded-none md:rounded-lg border-y md:border border-border bg-background shadow-sm flex flex-col">
      <div className="flex-1 overflow-auto custom-scrollbar">
        <table className="w-full text-sm text-left border-collapse">
          <thead className="sticky top-0 z-20 bg-muted/90 backdrop-blur-sm supports-[backdrop-filter]:bg-background/60 text-muted-foreground shadow-sm">
            <tr className="border-b border-border">
              {onSelectAll && (
                <th className="h-10 px-4 py-2 w-[40px] align-middle bg-muted/90 sticky left-0 z-30">
                  <Checkbox checked={allSelected} onChange={onSelectAll} />
                </th>
              )}
              {columns.map((col, i) => (
                <th key={i} className={`h-10 px-4 py-2 align-middle font-medium select-none whitespace-nowrap bg-muted/90 ${col.align === 'right' ? 'text-right' : col.align === 'center' ? 'text-center' : 'text-left'}`} style={{ width: col.width, minWidth: col.width || '150px' }}>
                  {col.headerName}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {isLoading ? (
              Array.from({ length: 10 }).map((_, i) => (
                  <tr key={i} className="h-12 animate-pulse bg-background">
                      <td colSpan={columns.length + (onSelectAll ? 1 : 0)} className="px-4 py-3">
                          <div className="h-4 w-full max-w-[120px] bg-secondary rounded" />
                      </td>
                  </tr>
              ))
            ) : data.length === 0 ? (
              <tr>
                <td colSpan={columns.length + (onSelectAll ? 1 : 0)} className="h-64 text-center text-muted-foreground">
                  <div className="flex flex-col items-center justify-center gap-2">
                    <span className="text-lg font-semibold">No records found</span>
                    <span className="text-sm opacity-70">Try adjusting your filters or add a new record.</span>
                  </div>
                </td>
              </tr>
            ) : (
              data.map((row) => {
                const id = String(row[keyField]);
                const isSelected = selectedIds.includes(id);
                return (
                  <tr 
                    key={id} 
                    onClick={() => onRowClick?.(row)}
                    className={`
                      group transition-colors hover:bg-muted/50 cursor-pointer h-12 border-l-2 border-l-transparent
                      ${isSelected ? 'bg-primary/5 hover:bg-primary/10 border-l-primary' : 'bg-background'}
                    `}
                  >
                    {onSelectRow && (
                      <td className="px-4 w-[40px] sticky left-0 bg-background group-hover:bg-muted/50 z-10 border-r border-border/50 md:border-none" onClick={e => e.stopPropagation()}>
                        <Checkbox checked={isSelected} onChange={() => onSelectRow(id)} />
                      </td>
                    )}
                    {columns.map((col, i) => (
                      <td key={i} className={`px-4 py-2 whitespace-nowrap ${col.align === 'right' ? 'text-right' : col.align === 'center' ? 'text-center' : 'text-left'}`}>
                        {col.renderCell ? col.renderCell(row) : (
                          <span className="text-foreground/90">{String(row[col.field as string] ?? '')}</span>
                        )}
                      </td>
                    ))}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
