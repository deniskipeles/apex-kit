
import React, { useState, useEffect } from 'react';
import { Plus, Database, User, Edit } from 'lucide-react';
import { Button, Card, CardHeader, CardTitle, CardContent, Badge, Skeleton } from '../../../components/form/FormPrimitives';
import { collectionsService } from '../services/collectionsService';
import { Collection, CollectionType } from '../../../types';
import { useCollectionsStore } from '../../../store/useCollectionsStore';

interface CollectionsListPageProps {
  onCreate: () => void;
  onEdit: () => void;
}

export const CollectionsListPage = ({ onCreate, onEdit }: CollectionsListPageProps) => {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [loading, setLoading] = useState(true);
  const { setActiveCollection } = useCollectionsStore();

  useEffect(() => {
    collectionsService.list().then(setCollections).finally(() => setLoading(false));
  }, []);

  const handleEdit = (col: Collection) => {
    setActiveCollection(col);
    onEdit();
  };

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
      </div>

      {loading ? (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          {[1,2,3].map(i => <Skeleton key={i} className="h-32 w-full" />)}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          {collections.map(col => (
            <Card key={col.id} className="group relative overflow-hidden hover:border-primary/50 transition-colors">
              <CardHeader>
                <div className="flex justify-between items-start">
                  <CardTitle className="flex items-center gap-2">
                    {col.type === CollectionType.AUTH ? <User className="h-4 w-4 text-orange-400" /> : <Database className="h-4 w-4 text-primary" />}
                    {col.name}
                  </CardTitle>
                  <div className="flex gap-2">
                    <Badge variant="secondary" className="uppercase text-[10px]">{col.type}</Badge>
                    <Button variant="ghost" size="icon" className="h-6 w-6 -mr-2" onClick={(e) => { e.stopPropagation(); handleEdit(col); }}>
                       <Edit className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent>
                <div className="text-sm text-muted-foreground space-y-1">
                  <p>{col.schema.length} fields defined</p>
                  <p>Updated {new Date(col.updated).toLocaleDateString()}</p>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
};