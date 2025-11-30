
import React, { useState, useEffect, useCallback, useRef } from 'react';
import { Plus, Edit, Trash2, FileText, Link as LinkIcon, Calendar, Check, X as XIcon, Database, MoreVertical, Filter, ChevronDown, Code, Info } from 'lucide-react';
import { Button, Badge } from '../../../components/form/FormPrimitives';
import { DataGrid } from '../../../components/data/DataGrid';
import { Pagination } from '../../../components/data/Pagination';
import { PreviewPanel } from '../../../components/preview/PreviewPanel';
import { RecordForm } from '../components/RecordForm';
import { RecordFilters } from '../components/RecordFilters';
import { ApiDocsModal } from '../components/ApiDocsModal';
import { collectionsService } from '../../collections/services/collectionsService';
import { recordsService } from '../services/recordsService';
import { AppRecord, Collection } from '../../../types';
import { Overlay } from '../../../components/overlay/Overlay';
import { usePagination } from '../../../hooks/usePagination';
import { InstantSearchInput } from '../../../components/search/InstantSearchInput';

const MobileCollectionSelect = ({ collections, active, onSelect }: { collections: Collection[], active: string, onSelect: (name: string) => void }) => {
  const [isOpen, setIsOpen] = useState(false);
  const triggerRef = useRef(null);

  return (
    <div className="md:hidden w-full">
      <button ref={triggerRef} onClick={() => setIsOpen(true)} className="flex w-full items-center justify-between gap-2 rounded-md border border-border bg-background px-3 py-2 text-sm font-bold shadow-sm">
        <span className="truncate flex items-center gap-2"><Database className="h-4 w-4 text-primary" /> {active || 'Select Collection'}</span>
        <ChevronDown className="h-4 w-4 opacity-50" />
      </button>
      <Overlay isOpen={isOpen} onClose={() => setIsOpen(false)} anchorRef={triggerRef} width="100%" align="start">
        <div className="flex flex-col rounded-md border bg-popover p-1 shadow-xl max-h-[300px] overflow-y-auto">
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

export const RecordsListPage = () => {
  const [records, setRecords] = useState<AppRecord[]>([]);
  const [cols, setCols] = useState<Collection[]>([]);
  const [activeCol, setActiveCol] = useState('');
  const [collection, setCollection] = useState<Collection | null>(null);
  const [viewMode, setViewMode] = useState<'list' | 'create' | 'edit'>('list');
  const [selectedRec, setSelectedRec] = useState<AppRecord | null>(null);
  const [previewRec, setPreviewRec] = useState<AppRecord | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isFilterOpen, setIsFilterOpen] = useState(false);
  const [isApiDocsOpen, setIsApiDocsOpen] = useState(false);

  // Pagination State
  const { page, setPage, perPage } = usePagination(1, 20);
  const [totalItems, setTotalItems] = useState(0);

  const fetchCollections = useCallback(async () => {
    const c = await collectionsService.list();
    setCols(c);
    if (c.length > 0 && !activeCol) {
      setActiveCol(c[0].name);
    }
  }, [activeCol]);

  const fetchRecords = useCallback(async () => {
    if (!activeCol) return;
    setIsLoading(true);
    const target = cols.find(c => c.name === activeCol);
    if (target) {
      setCollection(target);
      try {
        // 1. AUTO-CALCULATE EXPANSION STRING
        // Look for all fields of type 'relation' and join them with commas
        const expandStr = target.schema
          .filter(f => f.type === 'relation')
          .map(f => f.name)
          .join(',');

        // 2. PASS TO SERVICE
        const res = await recordsService.list(target.id, page, perPage, expandStr);
        setRecords(res.items);
        setTotalItems(res.totalItems);
      } catch (error) {
        console.error("Failed to fetch records", error);
      }
    }
    setIsLoading(false);
  }, [activeCol, cols, page, perPage]);

  // Reset page when switching collections
  useEffect(() => {
    setPage(1);
  }, [activeCol, setPage]);

  useEffect(() => {
    fetchCollections();
  }, [fetchCollections]);

  useEffect(() => {
    fetchRecords();
  }, [fetchRecords]);

  const handleSave = async (data: any) => {
    if (!collection) return;
    if (viewMode === 'edit' && selectedRec) {
      await recordsService.update(collection.id, selectedRec.id, data);
    } else {
      await recordsService.create(collection.id, data);
    }
    fetchRecords();
    setViewMode('list');
  };

  // Helper to fetch a single record when selected from Instant Search
  const handleInstantSelect = async (recordId: string) => {
    if (!collection) return;
    // We set it as previewRec to open the panel
    // However, previewRec expects a full AppRecord object. 
    // We can quickly fetch it or create a partial one if we trust the ID.
    // Ideally, fetch the fresh data:
    try {
      // You might need to expose a 'getOne' in recordsService for this, 
      // or filter from existing 'records' list if present.
      // Let's simulate a fetch via list filtering for now, or just set ID 
      // if you update PreviewPanel to fetch data itself.

      // Better approach: Just set what we know, PreviewPanel handles display
      const placeholderRecord: any = {
        id: recordId,
        collectionId: collection.id,
        collectionName: collection.name,
        created: new Date().toISOString(),
        updated: new Date().toISOString(),
        // We don't have full data yet, Preview might look empty until fully implemented
      };
      setPreviewRec(placeholderRecord);

      // Optionally: Trigger a refresh or fetch specific record logic here
    } catch (e) {
      console.error(e);
    }
  };

  const columns = [
    {
      field: 'id',
      headerName: 'ID',
      width: '120px',
      renderCell: (r: AppRecord) => (
        <div className="flex items-center gap-2 group">
          <span className="font-mono text-[10px] text-muted-foreground bg-secondary/50 px-1.5 py-0.5 rounded group-hover:text-primary transition-colors truncate max-w-[100px] border border-transparent group-hover:border-primary/20" title={r.id}>
            #{r.id}
          </span>
        </div>
      )
    },
    ...(collection?.schema.slice(0, 5).map(f => ({
      field: f.name,
      headerName: f.name,
      width: '150px',
      renderCell: (r: AppRecord) => {
        const val = r[f.name];
        if (val === undefined || val === null || val === '') return <span className="text-muted-foreground/30 text-xs italic">empty</span>;
        switch (f.type) {
          case 'bool': return val ? <Badge variant="success" className="h-5 px-1.5 text-[10px] gap-1"><Check className="h-3 w-3" /> True</Badge> : <Badge variant="secondary" className="h-5 px-1.5 text-[10px] gap-1 opacity-70"><XIcon className="h-3 w-3" /> False</Badge>;
          case 'date': return <span className="text-xs font-medium text-foreground/80">{new Date(val).toLocaleDateString()}</span>;
          case 'relations':
            // 3. GET EXPANDED DATA
            // The backend puts the object inside r.expand[fieldName]
            // Since r.expand might be an array (One-to-Many) or Object (One-to-One), we handle both for display
            const expandedData = r.expand ? r.expand[f.name] : null;

            return (
              <div className="flex items-center gap-2 group relative">
                <Badge variant="outline" className="font-mono text-[10px] h-5 border-primary/20 text-primary bg-primary/5 hover:bg-primary/10 cursor-pointer truncate max-w-[120px]">
                  {String(val)}
                </Badge>

                {/* INFO BUTTON & POPUP */}
                {expandedData && (
                  <div className="relative">
                    <button className="text-muted-foreground hover:text-primary opacity-50 group-hover:opacity-100 transition-opacity">
                      <Info className="h-3.5 w-3.5" />
                    </button>

                    {/* JSON POPUP (Pure CSS Hover) */}
                    <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-64 hidden group-hover:block z-50">
                      <div className="bg-popover border border-border text-popover-foreground rounded-lg shadow-xl overflow-hidden animate-in zoom-in-95 duration-200">
                        <div className="bg-secondary/20 px-3 py-1.5 border-b border-border flex justify-between items-center">
                          <span className="text-[10px] font-bold uppercase tracking-wider">Expanded Data</span>
                          <Code className="h-3 w-3 opacity-50" />
                        </div>
                        <div className="p-3 max-h-[200px] overflow-y-auto custom-scrollbar bg-[#1e1e1e]">
                          <pre className="text-[10px] font-mono text-blue-100 whitespace-pre-wrap leading-relaxed">
                            {JSON.stringify(expandedData, null, 2)}
                          </pre>
                        </div>
                      </div>
                      {/* Arrow */}
                      <div className="w-2 h-2 bg-border rotate-45 absolute -bottom-1 left-1/2 -translate-x-1/2 border-b border-r border-border bg-popover"></div>
                    </div>
                  </div>
                )}
              </div>
            );
          case 'json': return <code className="text-[10px] font-mono bg-muted px-1.5 py-0.5 rounded text-muted-foreground border border-border truncate max-w-[120px] block">{JSON.stringify(val).substring(0, 25)}</code>;
          case 'url': return <a href={String(val)} target="_blank" rel="noreferrer" onClick={e => e.stopPropagation()} className="text-primary hover:underline text-xs flex items-center gap-1 truncate max-w-[150px]"><LinkIcon className="h-3 w-3 flex-shrink-0" /> <span className="truncate">{String(val).replace(/(^\w+:|^)\/\//, '')}</span></a>;
          case 'file': return <div className="flex items-center gap-1.5 text-xs bg-secondary/30 px-2 py-1 rounded-md w-fit border border-transparent hover:border-primary/20 transition-colors"><FileText className="h-3 w-3 text-primary" /> <span className="truncate max-w-[100px] font-medium">{String(val)}</span></div>;
          case 'email': return <div className="text-xs flex items-center gap-1.5 truncate"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400 flex-shrink-0"></span>{String(val)}</div>;
          default: return <span className="text-sm truncate block max-w-[200px] text-foreground/90" title={String(val)}>{String(val)}</span>;
        }
      }
    })) || []),
    { field: 'updated', headerName: 'Updated', width: '140px', align: 'right' as const, renderCell: (r: AppRecord) => <span className="text-xs text-muted-foreground">{new Date(r.updated).toLocaleDateString()}</span> },
    { field: 'actions', headerName: '', width: '40px', align: 'right' as const, renderCell: () => <Button variant="ghost" size="icon" className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"><MoreVertical className="h-3 w-3" /></Button> }
  ];

  return (
    <div className="flex flex-col h-[calc(100vh-64px)] w-full md:flex-row bg-background/50">
      <div className="w-60 border-r bg-background/50 backdrop-blur supports-[backdrop-filter]:bg-background/60 hidden md:flex flex-col flex-shrink-0">
        <div className="h-14 flex items-center px-4 border-b"><h3 className="text-xs font-bold uppercase text-muted-foreground tracking-wider flex items-center gap-2"><Database className="h-3.5 w-3.5" /> Collections</h3></div>
        <div className="flex-1 overflow-y-auto p-3 space-y-1">
          {cols.map(c => <button key={c.id} onClick={() => setActiveCol(c.name)} className={`w-full text-left px-3 py-2 rounded-md text-sm font-medium transition-all flex items-center justify-between group ${activeCol === c.name ? 'bg-primary/10 text-primary shadow-sm' : 'hover:bg-secondary/80 text-muted-foreground hover:text-foreground'}`}><span className="truncate">{c.name}</span>{activeCol === c.name && <div className="h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_8px_rgba(var(--primary))]"></div>}</button>)}
        </div>
        <div className="p-3 border-t"><Button variant="outline" className="w-full text-xs justify-start" size="sm"><Plus className="mr-2 h-3 w-3" /> New Collection</Button></div>
      </div>

      <div className="flex-1 flex flex-col overflow-hidden relative min-w-0">
        <div className="h-auto min-h-16 border-b px-4 py-3 flex flex-col sm:flex-row sm:items-center justify-between gap-3 bg-background/80 backdrop-blur-md z-10">
          <div className="flex-1 w-full sm:w-auto">
            <div className="hidden md:block">
              <div className="flex items-center gap-3"><h2 className="font-bold text-lg truncate">{collection?.name}</h2>{totalItems > 0 && <Badge variant="secondary" className="text-[10px] h-5 px-1.5 font-mono flex-shrink-0">{totalItems} total</Badge>}</div>
              <span className="text-[10px] text-muted-foreground mt-0.5">Manage records for the {collection?.name} collection</span>
            </div>
            <div className="md:hidden w-full">
              <MobileCollectionSelect collections={cols} active={activeCol} onSelect={setActiveCol} />
            </div>
          </div>
          <div className="flex items-center gap-2 self-end sm:self-auto w-full sm:w-auto justify-end">
            {collection && (
              <InstantSearchInput
                collectionId={collection.id}
                onSelect={handleInstantSelect}
              />
            )}
            <Button variant="outline" size="sm" className="h-8 text-xs flex-1 sm:flex-none" onClick={() => setIsFilterOpen(true)}><Filter className="mr-2 h-3 w-3" /> Filter</Button>
            <Button size="sm" onClick={() => { setSelectedRec(null); setViewMode('create'); }} className="h-8 text-xs shadow-sm flex-1 sm:flex-none"><Plus className="mr-1.5 h-3.5 w-3.5" /> New Record</Button>
          </div>
        </div>

        <div className="flex-1 flex flex-col overflow-hidden p-0 sm:p-4 md:p-6 bg-background sm:bg-secondary/5">
          <div className="flex-1 overflow-hidden">
            <DataGrid data={records} columns={columns} keyField="id" isLoading={isLoading} onRowClick={(row) => setPreviewRec(row)} />
          </div>
          <div className="flex-shrink-0 pt-4 px-4 sm:px-0 flex justify-between items-center border-t border-border mt-2 bg-background sm:bg-transparent">
            <Button
              variant="ghost"
              size="sm"
              className="text-xs text-muted-foreground hover:text-primary gap-2"
              onClick={() => setIsApiDocsOpen(true)}
            >
              <Code className="h-4 w-4" />
              <span className="hidden sm:inline">API Docs</span>
            </Button>
            <Pagination
              page={page}
              totalPages={Math.ceil(totalItems / perPage) || 1}
              onPageChange={setPage}
            />
          </div>
        </div>
      </div>

      {(viewMode === 'create' || viewMode === 'edit') && collection && <RecordForm collection={collection} record={selectedRec || undefined} onSave={handleSave} onCancel={() => setViewMode('list')} />}

      <RecordFilters isOpen={isFilterOpen} onClose={() => setIsFilterOpen(false)} collection={collection} onApplyFilters={() => { }} />

      {collection && (
        <ApiDocsModal
          isOpen={isApiDocsOpen}
          onClose={() => setIsApiDocsOpen(false)}
          collection={collection}
        />
      )}

      <PreviewPanel isOpen={!!previewRec}
        onClose={() => setPreviewRec(null)}
        title="Record Details"
        actions={
          <>
            <Button className="flex-1"
              variant="outline"
              onClick={() => {
                setSelectedRec(previewRec);
                setPreviewRec(null);
                setViewMode('edit');
              }}>
              <Edit className="mr-2 h-4 w-4" />
              Edit
            </Button>
            <Button variant="destructive" size="icon"
              onClick={() => {
                recordsService.delete(previewRec!.collectionId, previewRec!.id)
                  .then(() => fetchRecords())
                  .then(() => setSelectedRec(null))
                  .catch(e => console.error(e));
              }}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </>
        }>
        <div className="space-y-6">
          <div className="grid grid-cols-2 gap-4 p-4 bg-secondary/10 rounded-lg border border-border">
            <div><span className="text-[10px] uppercase tracking-wider text-muted-foreground font-semibold">Created</span><p className="text-xs font-mono mt-1">{previewRec && new Date(previewRec.created).toLocaleString()}</p></div>
            <div><span className="text-[10px] uppercase tracking-wider text-muted-foreground font-semibold">Updated</span><p className="text-xs font-mono mt-1">{previewRec && new Date(previewRec.updated).toLocaleString()}</p></div>
            <div className="col-span-2 pt-2 border-t border-border/50"><span className="text-[10px] uppercase tracking-wider text-muted-foreground font-semibold">ID</span><p className="text-xs font-mono mt-1 select-all break-all">{previewRec?.id}</p></div>
          </div>
          <div className="space-y-4">
            {collection?.schema.map(f => (<div key={f.name}><div className="flex items-center gap-2 mb-1.5"><span className="text-xs font-bold text-muted-foreground uppercase tracking-wider">{f.name}</span><Badge variant="secondary" className="text-[8px] h-4 px-1">{f.type}</Badge></div><div className="p-3 bg-background rounded-md border text-sm break-words shadow-sm">{previewRec ? (typeof previewRec[f.name] === 'object' ? <pre className="text-[10px] whitespace-pre-wrap font-mono">{JSON.stringify(previewRec[f.name], null, 2)}</pre> : String(previewRec[f.name] ?? '-')) : '-'}</div></div>))}
          </div>
        </div>
      </PreviewPanel>
    </div>
  );
};
