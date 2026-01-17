import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  Plus, Edit, Trash2, FileText, Link as LinkIcon, Check, X as XIcon,
  Database, MoreVertical, Filter, ChevronDown, Code, Fingerprint,
  Upload, Download, Zap, MoreHorizontal, ArrowRight
} from 'lucide-react';
import { Button, Badge, Skeleton } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { Pagination } from '../../../components/data/Pagination';
import { PreviewPanel } from '../../../components/preview/PreviewPanel';
import { RecordForm } from '../components/RecordForm';
import { RecordFilters } from '../components/RecordFilters';
import { ApiDocsModal } from '../components/ApiDocsModal';
import { collectionsService } from '../../collections/services/collectionsService';
import { recordsService } from '../services/recordsService';
import { AppRecord, AppVersions, Collection } from '../../../types';
import { Overlay } from '../../../components/overlay/Overlay';
import { usePagination } from '../../../hooks/usePagination';
import { InstantSearchInput } from '../../../components/search/InstantSearchInput';
import { apiClient } from '@/src/lib/apiClient';
import { useToast } from '@/src/components/feedback/Toast';
import { Dialog } from '../../../components/ui/Dialog';
import { Input, Label } from '../../../components/form/FormPrimitives';
import { APEX_NUMBER_OF_RECORD_FIELDS, APEX_TRUNCATION_SIZE } from '@/src/constants';
import { VersionsModal } from '@/src/components/feedback/VersionsModal';
import { FileThumbnail } from '../../../components/media/FileThumbnail'; // [NEW] Import
import { RecordPreviewPanel } from '../components/RecordPreviewPanel';

// --- HELPER: Mobile Collection Selector ---
const MobileCollectionSelect = ({ collections, active, onSelect }: { collections: Collection[], active: string, onSelect: (name: string) => void }) => {
  const [isOpen, setIsOpen] = useState(false);
  const triggerRef = useRef(null);

  return (
    <div className="md:hidden w-full relative">
      <button ref={triggerRef} onClick={() => setIsOpen(!isOpen)} className="flex w-full items-center justify-between gap-2 rounded-md border border-border bg-background px-3 py-2 text-sm font-bold shadow-sm">
        <span className="truncate flex items-center gap-2"><Database className="h-4 w-4 text-primary" /> {active || 'Select Collection'}</span>
        <ChevronDown className="h-4 w-4 opacity-50" />
      </button>
      <Overlay isOpen={isOpen} onClose={() => setIsOpen(false)} anchorRef={triggerRef} width="100%" align="start">
        <div className="flex flex-col rounded-md border border-border bg-popover p-1 shadow-xl max-h-[300px] overflow-y-auto">
          {collections.map(c => (
            <button key={c.id} onClick={() => { onSelect(c.name); setIsOpen(false); }} className={`w-full rounded-sm px-3 py-2.5 text-left text-sm truncate flex items-center gap-2 ${active === c.name ? 'bg-accent font-semibold text-primary' : 'hover:bg-accent'}`}>
              {c.name}
            </button>
          ))}
        </div>
      </Overlay>
    </div>
  );
};

// --- HELPER: Action Menu ---
const ActionMenu = ({
  onReindex, onRevectorize, onImport, onExport
}: {
  onReindex: () => void, onRevectorize: () => void, onImport: () => void, onExport: () => void
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const triggerRef = useRef(null);

  return (
    <>
      <Button ref={triggerRef} variant="outline" size="icon" className="h-8 w-8 shrink-0" onClick={() => setIsOpen(!isOpen)}>
        <MoreHorizontal className="h-4 w-4" />
      </Button>
      <Overlay isOpen={isOpen} onClose={() => setIsOpen(false)} anchorRef={triggerRef} align="end" width={200} className="bg-popover border border-border shadow-xl rounded-md p-1 z-50">
        <div className="flex flex-col text-sm">
          <div className="px-2 py-1.5 text-[10px] font-bold text-muted-foreground uppercase tracking-wider">Data Operations</div>
          <button onClick={() => { onImport(); setIsOpen(false); }} className="flex items-center gap-2 px-2 py-2 hover:bg-accent rounded-sm text-left"><Upload className="h-3.5 w-3.5" /> Import Data</button>
          <button onClick={() => { onExport(); setIsOpen(false); }} className="flex items-center gap-2 px-2 py-2 hover:bg-accent rounded-sm text-left"><Download className="h-3.5 w-3.5" /> Export Data</button>
          <div className="my-1 border-b border-border"></div>
          <div className="px-2 py-1.5 text-[10px] font-bold text-muted-foreground uppercase tracking-wider">Maintenance</div>
          <button onClick={() => { onReindex(); setIsOpen(false); }} className="flex items-center gap-2 px-2 py-2 hover:bg-accent rounded-sm text-left"><Fingerprint className="h-3.5 w-3.5" /> Re-Index Search</button>
          <button onClick={() => { onRevectorize(); setIsOpen(false); }} className="flex items-center gap-2 px-2 py-2 hover:bg-accent rounded-sm text-left"><Zap className="h-3.5 w-3.5" /> Re-Vectorize AI</button>
        </div>
      </Overlay>
    </>
  );
};

// [NEW] Helper to guess mime type from filename for thumbnails
const guessMimeType = (filename: string): string => {
  const ext = filename.split('.').pop()?.toLowerCase();
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(ext || '')) return 'image/' + ext;
  if (['pdf'].includes(ext || '')) return 'application/pdf';
  return 'application/octet-stream';
};

export const RecordsListPage = () => {
  // State
  const [records, setRecords] = useState<AppRecord[]>([]);
  const [cols, setCols] = useState<Collection[]>([]);
  const [activeCol, setActiveCol] = useState('');
  const [collection, setCollection] = useState<Collection | null>(null);

  const [viewMode, setViewMode] = useState<'list' | 'create' | 'edit'>('list');
  const [selectedRec, setSelectedRec] = useState<AppRecord | null>(null);
  const [previewRec, setPreviewRec] = useState<AppRecord | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  // Search & Filter State
  const [searchText, setSearchText] = useState('');
  const [activeFilters, setActiveFilters] = useState<any>({});

  // Modals
  const [isFilterOpen, setIsFilterOpen] = useState(false);
  const [isApiDocsOpen, setIsApiDocsOpen] = useState(false);
  const [isImportOpen, setIsImportOpen] = useState(false);
  const [isImporting, setIsImporting] = useState(false); // [NEW] Loading state for import

  // Pagination
  const { page, setPage, perPage } = usePagination(1, 20);
  const [totalItems, setTotalItems] = useState(0);

  // Version State
  const [versions, setVersions] = useState<AppVersions | null>(null);
  const [isVersionsOpen, setIsVersionsOpen] = useState(false);

  const { toast } = useToast();

  // Fetch Versions on mount
  useEffect(() => {
    apiClient.getVersions().then(setVersions);
  }, []);

  // 1. Initial Load: Fetch Collections
  useEffect(() => {
    const init = async () => {
      try {
        const c = await collectionsService.list();
        setCols(c);
        if (c.length > 0 && !activeCol) {
          setActiveCol(c[0].name);
          setCollection(c[0]);
        }
      } catch (e) {
        toast('Failed to load collections', 'error');
      }
    };
    init();
  }, []); // Run once

  // 2. Main Data Fetcher
  const loadData = useCallback(async () => {
    if (!activeCol) return;

    const targetCol = cols.find(c => c.name === activeCol);
    if (!targetCol) return;

    setCollection(targetCol);
    setIsLoading(true);

    try {
      // A. Standard List (Pagination + Filtering)
      if (!searchText) {
        const expandStr = targetCol.schema
          .filter(f => f.type === 'relation' || f.type === "owner")
          .map(f => f.name)
          .join(',');

        const res = await recordsService.list(targetCol.id, page, perPage, expandStr, activeFilters, "-id");
        setRecords(res.items);
        setTotalItems(res.totalItems);
      }
      // B. Deep Search (Enter Key Hit)
      else {
        // Note: This hits the DB search logic (e.g. SQL LIKE or Vector if implemented in backend)
        const res = await recordsService.searchRecords(targetCol.id, searchText);
        setRecords(res);
        setTotalItems(res.length); // Assuming flat list return for search
      }
    } catch (e) {
      console.error(e);
      toast('Failed to load records', 'error');
    } finally {
      setIsLoading(false);
    }
  }, [activeCol, cols, page, perPage, searchText, activeFilters]);

  // 3. Trigger Load
  useEffect(() => {
    loadData();
  }, [loadData]);

  // --- Handlers ---

  const handleCollectionChange = (name: string) => {
    if (name === activeCol) return;
    setActiveCol(name);
    setPage(1);
    setSearchText('');
    setActiveFilters({});
  };

  // Called by InstantSearchInput on Enter
  const handleDeepSearch = (query: string) => {
    setSearchText(query);
    setPage(1);
  };

  // Called by InstantSearchInput on selection
  const handleInstantSelect = async (recordId: string) => {
    if (!collection) return;
    try {
      const rec = await recordsService.getOne(collection.id, recordId);
      setPreviewRec(rec);
    } catch (e) {
      console.error(e);
    }
  };

  const handleApplyFilters = (filters: any) => {
    setActiveFilters(filters);
    setPage(1);
    setIsFilterOpen(false);
  };

  const handleSave = async (data: any) => {
    if (!collection) return;
    try {
      if (viewMode === 'edit' && selectedRec) {
        await recordsService.update(collection.id, selectedRec.id, data);
        toast('Record updated', 'success');
      } else {
        await recordsService.create(collection.id, data);
        toast('Record created', 'success');
      }
      loadData();
      setViewMode('list');
    } catch (e) {
      toast('Operation failed', 'error');
    }
  };

  const handleReIndex = async () => {
    if (!collection) return;
    try {
      const res = await apiClient.reIndex(collection.id);
      toast(res.message || 'Re-index started', 'success');
    } catch (e: any) {
      toast('Re-index failed', 'error');
    }
  }

  const handleReVectorize = async () => {
    if (!collection) return;
    try {
      const res = await apiClient.revectorizeCollection(collection.id);
      if (res.success) toast('AI Vectorization started', 'success');
      else toast('Failed to start vectorization', 'error');
    } catch (e) {
      toast('Error triggering vectorization', 'error');
    }
  };

  const handleImport = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!collection) return;
    const form = e.target as HTMLFormElement;
    const fileInput = form.elements.namedItem('file') as HTMLInputElement;
    if (!fileInput.files?.length) return;

    setIsImporting(true); // Start loading
    try {
      const res = await apiClient.importData(collection.name, fileInput.files[0]);
      if (res.ok) {
        const json = await res.json();
        toast(`Imported ${json.records_imported} records`, 'success');
        if (json.schema_updated) {
          toast('Schema updated based on imported data', 'info');
        }
        setIsImportOpen(false);
        loadData();
      } else {
        toast('Import failed', 'error');
      }
    } catch (err) {
      toast('Network error during import', 'error');
    } finally {
      setIsImporting(false); // Stop loading
    }
  };

  const handleExport = async () => {
    if (!collection) return;
    await apiClient.exportData(collection.id, 'json')
      .then(blob => {
        const a = document.createElement('a');
        a.href = window.URL.createObjectURL(blob);
        a.download = `${collection.name}.json`;
        a.click();
      })
      .catch(() => toast('Export failed', 'error'));
  };

  // --- COLUMNS DEF ---
  const columns = [
    {
      field: 'id', headerName: 'ID', width: '80px',
      renderCell: (r: AppRecord) => (
        <span className="font-mono text-[10px] text-muted-foreground bg-secondary/50 px-1.5 py-0.5 rounded truncate border border-transparent hover:border-primary/20" title={r.id}>#{r.id}</span>
      )
    },
    ...(collection?.schema.slice(0, APEX_NUMBER_OF_RECORD_FIELDS).map(f => ({
      field: f.name,
      headerName: f.name,
      width: '150px',
      renderCell: (r: AppRecord) => {
        const val = r[f.name];
        if (val === undefined || val === null || val === '') return <span className="text-muted-foreground/30 text-xs italic">-</span>;

        switch (f.type) {
          case 'bool': return val ? <Badge variant="success" className="h-5 px-1.5 text-[10px] gap-1"><Check className="h-3 w-3" /> True</Badge> : <Badge variant="secondary" className="h-5 px-1.5 text-[10px] gap-1 opacity-70"><XIcon className="h-3 w-3" /> False</Badge>;
          case 'date': return <span className="text-xs font-medium text-foreground/80">{new Date(val).toLocaleDateString()}</span>;
          case 'relation':
          case 'owner': {
            const expanded = r.expand?.[f.name];
            let label = String(val);
            // Try to find a human-readable label from the expanded data
            if (expanded) {
              const getDisplay = (obj: any) => obj.data?.title || obj.data?.name || obj.data?.email || obj.data?.slug || obj.email || obj.id;
              if (Array.isArray(expanded)) {
                if (expanded.length > 0) {
                  const first = getDisplay(expanded[0]);
                  label = expanded.length > 1 ? `${first} (+${expanded.length - 1})` : first;
                }
              } else {
                label = getDisplay(expanded);
              }
            }

            return (
              <Badge variant="outline" className="font-mono text-[10px] h-5 border-primary/20 text-primary bg-primary/5 truncate max-w-[120px]" title={`ID: ${val}`}>
                {label}
              </Badge>
            );
          }
          case 'json': return <code className="text-[10px] font-mono bg-muted px-1.5 py-0.5 rounded text-muted-foreground border border-border truncate max-w-[120px] block">{JSON.stringify(val).substring(0, APEX_TRUNCATION_SIZE)}...</code>;
          case 'url': return <a href={String(val)} target="_blank" rel="noreferrer" onClick={e => e.stopPropagation()} className="text-primary hover:underline text-xs flex items-center gap-1 truncate max-w-[150px]"><LinkIcon className="h-3 w-3 flex-shrink-0" /> <span className="truncate">{String(val).replace(/(^\w+:|^)\/\//, '')}</span></a>;

          // [UPDATED] Render File Thumbnail
          case 'file': {
            const url = apiClient.files.getFileUrl(String(val));
            return (
              <div className="flex items-center gap-2 group">
                <div className="h-8 w-8 rounded overflow-hidden bg-secondary border border-border shrink-0">
                  <FileThumbnail url={url+"?thumb=100x100"} mimeType={guessMimeType(String(val))} />
                </div>
                <span className="truncate max-w-[100px] text-xs font-medium">{String(val)}</span>
              </div>
            );
          }

          case 'text': return <span className="text-xs text-muted-foreground line-clamp-1 max-w-[200px]" title={String(apiClient.stripHtmlTags(val))}>{String(apiClient.stripHtmlTags(val))}</span>;
          default: return <span className="text-sm truncate block max-w-[200px] text-foreground/90" title={String(val)}>{String(val)}</span>;
        }
      }
    })) || []),
    { field: 'updated', headerName: 'Updated', width: '120px', align: 'right' as const, renderCell: (r: AppRecord) => <span className="text-xs text-muted-foreground">{new Date(r.updated).toLocaleDateString()}</span> },
    { field: 'actions', headerName: '', width: '40px', align: 'right' as const, renderCell: () => <Button variant="ghost" size="icon" className="h-6 w-6 opacity-50 hover:opacity-100"><MoreVertical className="h-3 w-3" /></Button> }
  ];

  return (
    <div className="flex flex-col h-[calc(100vh-64px)] w-full md:flex-row bg-background/50">

      {/* SIDEBAR: Collections List (Desktop) */}
      <div className="w-60 border-r bg-background/50 backdrop-blur supports-[backdrop-filter]:bg-background/60 hidden md:flex flex-col flex-shrink-0">
        <div className="h-14 flex items-center px-4 border-b">
          <h3 className="text-xs font-bold uppercase text-muted-foreground tracking-wider flex items-center gap-2">
            <Database className="h-3.5 w-3.5" /> Collections
          </h3>
        </div>
        <div className="flex-1 overflow-y-auto p-3 space-y-1 custom-scrollbar">
          {cols.map(c => (
            <button
              key={c.id}
              onClick={() => handleCollectionChange(c.name)}
              className={`w-full text-left px-3 py-2 rounded-md text-sm font-medium transition-all flex items-center justify-between group ${activeCol === c.name ? 'bg-primary/10 text-primary shadow-sm' : 'hover:bg-secondary/80 text-muted-foreground hover:text-foreground'}`}
            >
              <span className="truncate">{c.name}</span>
              {activeCol === c.name && <div className="h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_8px_rgba(var(--primary))]"></div>}
            </button>
          ))}
        </div>
        <div className="p-3 border-t">
          <Button variant="outline" className="w-full text-xs justify-start" size="sm" onClick={() => { setActiveCol(''); setViewMode('create'); /* Or handle collection creation explicitly */ }}>
            <Plus className="mr-2 h-3 w-3" /> New Collection
          </Button>
        </div>
      </div>

      {/* MAIN CONTENT */}
      <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">

        {/* TOOLBAR */}
        <div className="h-auto min-h-16 border-b px-4 py-3 flex flex-col lg:flex-row lg:items-center justify-between gap-3 bg-background/80 backdrop-blur-md z-30">
          <div className="flex-1 w-full lg:w-auto">
            <div className="hidden md:block">
              <div className="flex items-center gap-3">
                <h2 className="font-bold text-lg truncate">{collection?.name || 'Select Collection'}</h2>
                {totalItems > 0 && <Badge variant="secondary" className="text-[10px] h-5 px-1.5 font-mono flex-shrink-0">{totalItems} total</Badge>}
              </div>
            </div>
            <div className="md:hidden w-full">
              <MobileCollectionSelect collections={cols} active={activeCol} onSelect={handleCollectionChange} />
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2 self-start sm:self-end lg:self-auto w-full lg:w-auto justify-end">

            {/* UNIFIED SEARCH INPUT */}
            {collection && (
              <div className="w-full sm:w-auto flex-1 sm:flex-none min-w-[240px]">
                <InstantSearchInput
                  collectionId={collection.id}
                  onSelect={handleInstantSelect}
                  onSearch={handleDeepSearch} // Connect Deep Search
                  placeholder="Search records..."
                />
              </div>
            )}

            <div className="hidden xl:flex gap-2">
              <Button variant="outline" size="sm" className="h-8 text-xs" onClick={() => setIsFilterOpen(true)}><Filter className="mr-2 h-3 w-3" /> Filter</Button>
              <Button variant="outline" size="sm" className="h-8 text-xs" onClick={() => setIsImportOpen(true)}><Upload className="mr-2 h-3 w-3" /> Import</Button>
              <Button variant="outline" size="sm" className="h-8 text-xs" onClick={handleExport}><Download className="mr-2 h-3 w-3" /> Export</Button>
            </div>

            <div className="xl:hidden">
              <Button variant="outline" size="icon" className="h-8 w-8" onClick={() => setIsFilterOpen(true)}><Filter className="h-3.5 w-3.5" /></Button>
            </div>

            <ActionMenu
              onReindex={handleReIndex}
              onRevectorize={handleReVectorize}
              onImport={() => setIsImportOpen(true)}
              onExport={handleExport}
            />

            <Button size="sm" onClick={() => { setSelectedRec(null); setViewMode('create'); }} className="h-8 text-xs shadow-sm whitespace-nowrap"><Plus className="mr-1.5 h-3.5 w-3.5" /> New Record</Button>
          </div>
        </div>

        {/* GRID */}
        <div className="flex-1 flex flex-col overflow-hidden p-0 sm:p-4 md:p-6 bg-background sm:bg-secondary/5">
          <div className="flex-1 overflow-hidden relative">
            {isLoading ? (
              <div className="absolute inset-0 flex items-center justify-center bg-background/50 z-10">
                <div className="h-8 w-8 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
              </div>
            ) : null}
            <DataGrid data={records} columns={columns} keyField="id" isLoading={isLoading && records.length === 0} onRowClick={(row) => setPreviewRec(row)} />
          </div>

          <div className="flex-shrink-0 pt-4 px-4 sm:px-0 flex justify-between items-center border-t border-border mt-2 bg-background sm:bg-transparent">
            {/* Left Side Footer */}
            <div className="flex items-center gap-3">
              <Button
                variant="ghost"
                size="sm"
                className="text-xs text-muted-foreground hover:text-primary gap-2"
                onClick={() => setIsApiDocsOpen(true)}
              >
                <Code className="h-4 w-4" />
                <span className="hidden sm:inline">API Docs</span>
              </Button>

              {/* [NEW] Version Indicator */}
              {versions && (
                <>
                  <div className="h-4 w-px bg-border"></div>
                  <button
                    onClick={() => setIsVersionsOpen(true)}
                    className="text-[10px] font-mono text-muted-foreground/60 hover:text-primary transition-colors cursor-pointer"
                    title="Click to view all module versions"
                  >
                    v{versions.root}
                  </button>
                </>
              )}
            </div>

            {/* Pagination works for both list and search */}
            <Pagination
              page={page}
              totalPages={Math.ceil(totalItems / perPage) || 1}
              onPageChange={setPage}
            />
          </div>
        </div>
      </div>

      {/* --- MODALS & PANELS --- */}
      {/* [NEW] Versions Modal */}
      <VersionsModal
        isOpen={isVersionsOpen}
        onClose={() => setIsVersionsOpen(false)}
        versions={versions}
      />

      {(viewMode === 'create' || viewMode === 'edit') && collection &&
        <RecordForm
          collection={collection}
          record={selectedRec || undefined}
          onSave={handleSave}
          onCancel={() => setViewMode('list')}
        />
      }

      <RecordFilters
        isOpen={isFilterOpen}
        onClose={() => setIsFilterOpen(false)}
        collection={collection}
        onApplyFilters={handleApplyFilters}
      />

      <ApiDocsModal
        isOpen={isApiDocsOpen}
        onClose={() => setIsApiDocsOpen(false)}
        collection={collection || undefined}
        context="collection"
      />

      <RecordPreviewPanel
        record={previewRec}
        collection={collection}
        isOpen={!!previewRec}
        onClose={() => setPreviewRec(null)}
        onEdit={() => { setSelectedRec(previewRec); setPreviewRec(null); setViewMode('edit'); }}
        onDelete={() => {
          if (previewRec) recordsService.delete(previewRec.collectionId, previewRec.id).then(() => { loadData(); setPreviewRec(null); });
        }}
      />

      {/* IMPORT MODAL */}
      <Dialog isOpen={isImportOpen} onClose={() => !isImporting && setIsImportOpen(false)} title="Import Data" size="sm">
        <form onSubmit={handleImport} className="space-y-4">
          <div className="text-sm text-muted-foreground">Upload a <b>JSON</b> or <b>CSV</b> file. Schema will be inferred if the collection is empty.</div>
          <div className="space-y-2">
            <Label>File</Label>
            <Input type="file" name="file" accept=".json,.csv" required className="cursor-pointer" disabled={isImporting} />
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="ghost" onClick={() => setIsImportOpen(false)} disabled={isImporting}>Cancel</Button>
            <Button type="submit" isLoading={isImporting} disabled={isImporting}>
              <Upload className="mr-2 h-4 w-4" /> Start Import
            </Button>
          </div>
        </form>
      </Dialog>
    </div>
  );
};
