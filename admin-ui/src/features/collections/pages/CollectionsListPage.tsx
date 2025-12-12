import React, { useState, useEffect } from 'react';
import { Plus, Database, User, Edit, Trash2, Fingerprint } from 'lucide-react';
import { Button, Card, CardHeader, CardTitle, CardContent, Badge, Skeleton } from '../../../components/ui/Elements';
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
  const [collectionToDelete, setCollectionToDelete] = useState<Collection | null>(null);
  const { setActiveCollection } = useCollectionsStore();
  const { toast } = useToast();

  useEffect(() => {
    fetchCollections();
  }, []);

  const fetchCollections = () => {
    setLoading(true);
    collectionsService.list()
      .then(setCollections)
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

  const reIndexAll = async () => {
    const completedIndexing = false;
    let collectionsCounter = 0;
    while (collectionsCounter < collections.length) {
      const collection = collections[collectionsCounter];
      if (collection) {
        const res = await apiClient.reIndex(collection.id);
        if (res.success) toast(res?.message + " " + collection.name, "success");
        if (!res.success) toast(res?.message + " " + collection.name, "error");
      }
      collectionsCounter = collectionsCounter + 1;
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-start justify-between gap-4 sm:flex-row sm:items-center">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Collections</h2>
          <p className="text-muted-foreground">Manage your data schema and access rules.</p>
        </div>
        <Button onClick={() => { setActiveCollection(null); onCreate(); }}>
          <Plus className="mr-2 h-4 w-4" /> New Collection
        </Button>
        <Button onClick={() => { reIndexAll() }}>
          <Fingerprint className="mr-2 h-4 w-4" /> Re-Index All
        </Button>
      </div>

      {loading ? (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          {[1, 2, 3].map(i => <Skeleton key={i} className="h-32 w-full" />)}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          {collections.map(col => (
            <Card key={col.id} className="group relative overflow-hidden hover:border-primary/50 transition-colors">
              <CardHeader>
                <div className="flex justify-between items-start">
                  <CardTitle className="flex items-center gap-2">
                    {col.type === 'auth' ? <User className="h-4 w-4 text-orange-400" /> : <Database className="h-4 w-4 text-primary" />}
                    {col.name}
                  </CardTitle>
                  <div className="flex gap-1 items-center">
                    <Badge variant="secondary" className="uppercase text-[10px] mr-1">{col.type}</Badge>

                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-muted-foreground hover:text-primary"
                      onClick={(e) => { e.stopPropagation(); handleEdit(col); }}
                      title="Edit Schema"
                    >
                      <Edit className="h-3.5 w-3.5" />
                    </Button>

                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-muted-foreground hover:text-destructive"
                      onClick={(e) => { e.stopPropagation(); setCollectionToDelete(col); }}
                      title="Delete Collection"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent>
                <div className="text-sm text-muted-foreground space-y-1">
                  <p>{col.schema.length} fields defined</p>
                  <p className="text-xs opacity-70">Updated {new Date(col.updated).toLocaleDateString()}</p>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <ConfirmDialog
        isOpen={!!collectionToDelete}
        title={`Delete ${collectionToDelete?.name}?`}
        description="This will permanently delete the collection and all its records. This action cannot be undone."
        confirmText="Delete Collection"
        variant="destructive"
        onConfirm={handleDelete}
        onCancel={() => setCollectionToDelete(null)}
      />
    </div>
  );
};