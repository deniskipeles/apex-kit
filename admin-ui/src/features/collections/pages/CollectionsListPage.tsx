import React, { useState, useEffect } from 'react';
import {
  Plus, Database, User, Edit, Trash2, Fingerprint,
  BrainCircuit, Search, Layers, ArrowRight,
  Download,
  Upload
} from 'lucide-react';
import {
  Button, Card, CardHeader,
  CardTitle, CardContent, Badge,
  Skeleton, Input, Select, Label
} from '@/src/components/ui/Elements';
import { Dialog } from '@/src/components/ui/Dialog';
import { collectionsService } from '../services/collectionsService';
import { Collection } from '../../../types';
import { useCollectionsStore } from '../../../store/useCollectionsStore';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '@/src/lib/apiClient';

interface CollectionsListPageProps {
  onCreate: () => void;
  onEdit: () => void;
}

export const CollectionsListPage = ({ onCreate, onEdit }: CollectionsListPageProps) => {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [collectionToDelete, setCollectionToDelete] = useState<Collection | null>(null);
  const [isReindexing, setIsReindexing] = useState(false);
  const [isRevectorizing, setIsRevectorizing] = useState(false);

  const [isExporting, setIsExporting] = useState(false);
  const [isImportModalOpen, setIsImportModalOpen] = useState(false);
  const [isImporting, setIsImporting] = useState(false);

  const { setActiveCollection } = useCollectionsStore();
  const { toast } = useToast();

  useEffect(() => {
    fetchCollections();
  }, []);

  const fetchCollections = () => {
    setLoading(true);
    collectionsService.list()
      .then(setCollections)
      .catch(() => toast('Failed to load collections', 'error'))
      .finally(() => setLoading(false));
  };

  const handleEdit = (col: Collection) => {
    setActiveCollection(col);
    onEdit();
  };

  const handleDelete = async () => {
    if (!collectionToDelete) return;
    try {
      await collectionsService.delete(collectionToDelete.id);
      setCollections(prev => prev.filter(c => c.id !== collectionToDelete.id));
      toast(`Collection "${collectionToDelete.name}" deleted`, 'success');
    } catch (e) {
      toast('Failed to delete collection', 'error');
    } finally {
      setCollectionToDelete(null);
    }
  };

  const handleReIndexAll = async () => {
    setIsReindexing(true);
    let count = 0;
    try {
      for (const col of collections) {
        // Only re-index if it has indexed fields
        if (col.schema.some(f => f.ose_indexed)) {
          await apiClient.reIndex(col.id);
          count++;
        }
      }
      toast(`Triggered re-indexing for ${count} collections`, 'success');
    } catch (e) {
      toast('Bulk re-indexing encountered errors', 'error');
    } finally {
      setIsReindexing(false);
    }
  };

  const handleRevectorizeAll = async () => {
    setIsRevectorizing(true);
    let count = 0;
    try {
      for (const col of collections) {
        // Only vectorizable collections
        if (col.schema.some(f => f.vectorize)) {
          await apiClient.revectorizeCollection(col.id);
          count++;
        }
      }
      toast(`Vectorization jobs started for ${count} collections`, 'success');
    } catch (e) {
      toast('Bulk vectorization failed', 'error');
    } finally {
      setIsRevectorizing(false);
    }
  };

  const filteredCollections = collections.filter(c =>
    c.name.toLowerCase().includes(search.toLowerCase())
  );

  const handleExportSchema = async () => {
    setIsExporting(true);
    try {
      await apiClient.collections.exportSchema();
      toast('Schema downloaded successfully', 'success');
    } catch (e: any) {
      console.error(e);
      toast(e.message || 'Failed to export schema', 'error');
    } finally {
      setIsExporting(false);
    }
  };

  const handleImportSchema = async (e: React.FormEvent) => {
    e.preventDefault();
    const form = e.target as HTMLFormElement;
    const file = (form.elements.namedItem('file') as HTMLInputElement).files?.[0];
    const strategy = (form.elements.namedItem('strategy') as HTMLSelectElement).value as any;

    if (!file) return;

    setIsImporting(true);
    try {
      const res = await apiClient.collections.importSchema(file, strategy);
      toast(`Schema Import: Created ${res.created}, Updated ${res.updated}, Skipped ${res.skipped}`, 'success');
      if (res.errors.length > 0) {
        console.warn(res.errors);
        toast('Some collections failed to import. Check console.', 'warning');
      }
      setIsImportModalOpen(false);
      fetchCollections(); // Refresh list
    } catch (e: any) {
      toast(e.message, 'error');
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <div className="space-y-8 pb-20 max-w-7xl mx-auto">
      {/* Header & Actions */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-6">
        <div className="space-y-1">
          <h2 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <Layers className="h-8 w-8 text-primary" /> Data Collections
          </h2>
          <p className="text-muted-foreground text-sm md:text-base">
            Define schemas, manage records, and configure AI search capabilities.
          </p>
        </div>

        <div className="flex flex-col sm:flex-row gap-3">
          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search collections..."
              className="pl-9 w-full sm:w-64 bg-background/50"
              value={search}
              onChange={(e: any) => setSearch(e.target.value)}
            />
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={handleExportSchema} isLoading={isExporting} title="Download Schema JSON">
              <Download className="mr-2 h-4 w-4" /> Export
            </Button>
            <Button variant="outline" onClick={() => setIsImportModalOpen(true)} title="Upload Schema JSON">
              <Upload className="mr-2 h-4 w-4" /> Import
            </Button>
            <Button onClick={() => { setActiveCollection(null); onCreate(); }} className="shadow-lg hover:shadow-primary/25 transition-all">
              <Plus className="mr-2 h-4 w-4" /> New Collection
            </Button>
          </div>
        </div>
      </div>

      {/* Maintenance Toolbar */}
      <div className="flex flex-wrap gap-2 p-1 bg-secondary/10 rounded-lg border border-border/50 w-fit">
        <Button
          variant="ghost"
          size="sm"
          onClick={handleReIndexAll}
          isLoading={isReindexing}
          className="text-xs text-muted-foreground hover:text-foreground"
        >
          <Fingerprint className="mr-2 h-3.5 w-3.5" /> Re-Index Search
        </Button>
        <div className="w-px bg-border my-1"></div>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleRevectorizeAll}
          isLoading={isRevectorizing}
          className="text-xs text-muted-foreground hover:text-foreground hover:bg-purple-500/10 hover:text-purple-400"
        >
          <BrainCircuit className="mr-2 h-3.5 w-3.5" /> Re-Vectorize All
        </Button>
      </div>

      {/* Grid */}
      {loading ? (
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3, 4, 5, 6].map(i => <Skeleton key={i} className="h-40 w-full rounded-xl" />)}
        </div>
      ) : filteredCollections.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-center border-2 border-dashed border-border rounded-xl bg-secondary/5">
          <div className="h-20 w-20 bg-secondary/30 rounded-full flex items-center justify-center mb-4">
            <Database className="h-10 w-10 text-muted-foreground/50" />
          </div>
          <h3 className="text-xl font-semibold mb-2">No Collections Found</h3>
          <p className="text-muted-foreground max-w-md mb-6">
            {search ? `No matches for "${search}"` : "Get started by creating your first data collection."}
          </p>
          {!search && (
            <Button variant="outline" onClick={() => { setActiveCollection(null); onCreate(); }}>
              Create Collection
            </Button>
          )}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {filteredCollections.map(col => {
            const hasVectors = col.schema.some(f => f.vectorize);
            const fieldCount = col.schema.length;
            const isAuth = col.type === 'auth';

            return (
              <Card
                key={col.id}
                className="group relative overflow-hidden transition-all duration-300 hover:shadow-xl hover:border-primary/50 hover:-translate-y-1 bg-card/50 backdrop-blur-sm"
              >
                <CardHeader className="pb-3">
                  <div className="flex justify-between items-start">
                    <div className="flex items-center gap-3">
                      <div className={`p-2 rounded-lg ${isAuth ? 'bg-orange-500/10 text-orange-500' : 'bg-primary/10 text-primary'}`}>
                        {isAuth ? <User className="h-5 w-5" /> : <Database className="h-5 w-5" />}
                      </div>
                      <div>
                        <CardTitle className="text-lg font-bold">{col.name}</CardTitle>
                        <span className="text-[10px] text-muted-foreground uppercase tracking-wider font-semibold">
                          {col.type}
                        </span>
                      </div>
                    </div>

                    <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-muted-foreground hover:text-destructive"
                        onClick={(e) => { e.stopPropagation(); setCollectionToDelete(col); }}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                </CardHeader>

                <CardContent>
                  <div className="space-y-4">
                    {/* Stats */}
                    <div className="flex gap-2">
                      <Badge variant="secondary" className="text-[10px] font-mono">
                        {fieldCount} Fields
                      </Badge>
                      {hasVectors && (
                        <Badge variant="secondary" className="text-[10px] font-mono bg-purple-500/10 text-purple-400 border-purple-500/20">
                          <BrainCircuit className="h-3 w-3 mr-1" /> AI Ready
                        </Badge>
                      )}
                    </div>

                    {/* Action */}
                    <Button
                      variant="outline"
                      className="w-full justify-between group-hover:border-primary/30 group-hover:bg-primary/5"
                      onClick={() => handleEdit(col)}
                    >
                      <span className="text-xs">Edit Schema</span>
                      <ArrowRight className="h-3 w-3 text-muted-foreground group-hover:text-primary transition-colors" />
                    </Button>
                  </div>

                  {/* Timestamp Footer */}
                  <div className="absolute bottom-2 right-3 text-[9px] text-muted-foreground/30 font-mono">
                    Updated {new Date(col.updated).toLocaleDateString()}
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      {/* NEW IMPORT MODAL */}
      <Dialog
        isOpen={isImportModalOpen}
        onClose={() => !isImporting && setIsImportModalOpen(false)}
        title="Import Schema"
        size="sm"
      >
        <form onSubmit={handleImportSchema} className="space-y-4">
          <div className="text-sm text-muted-foreground">
            Upload an <code>apex_schema.json</code> file to restore or migrate collections structure.
          </div>

          <div className="space-y-2">
            <Label>JSON File</Label>
            <Input type="file" name="file" accept=".json" required disabled={isImporting} />
          </div>

          <div className="space-y-2">
            <Label>Conflict Strategy</Label>
            <Select name="strategy" defaultValue="skip" disabled={isImporting}>
              <option value="skip">Skip existing collections</option>
              <option value="overwrite">Overwrite existing schema</option>
              <option value="error">Fail if exists</option>
            </Select>
            <p className="text-[10px] text-muted-foreground">
              "Overwrite" will update fields/rules but preserve existing records (unless fields are removed).
            </p>
          </div>

          <div className="flex justify-end gap-2 pt-2 border-t border-border">
            <Button type="button" variant="ghost" onClick={() => setIsImportModalOpen(false)} disabled={isImporting}>
              Cancel
            </Button>
            <Button type="submit" isLoading={isImporting} disabled={isImporting}>
              <Upload className="mr-2 h-4 w-4" /> Start Import
            </Button>
          </div>
        </form>
      </Dialog>

      <ConfirmDialog
        isOpen={!!collectionToDelete}
        title={`Delete ${collectionToDelete?.name}?`}
        description="This will permanently delete the collection, all its records, and search indexes. This action cannot be undone."
        confirmText="Delete Collection"
        variant="destructive"
        onConfirm={handleDelete}
        onCancel={() => setCollectionToDelete(null)}
      />
    </div>
  );
};