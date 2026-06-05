import React, { Suspense } from 'react';
import { useCollectionsStore } from '../../../store/useCollectionsStore';
import { Loader2 } from 'lucide-react';

// Dynamic Import
const CollectionForm = React.lazy(() =>
  import('../components/CollectionCreator').then((module) => ({ default: module.CollectionForm }))
);

interface CollectionEditPageProps {
  onCancel: () => void;
  onSuccess: () => void;
}

export const CollectionEditPage = ({ onCancel, onSuccess }: CollectionEditPageProps) => {
  const { activeCollection, updateCollection } = useCollectionsStore();

  const handleSave = async (data: any) => {
    if (activeCollection) {
      await updateCollection(activeCollection.id, data);
      onSuccess();
    }
  };

  if (!activeCollection) {
    onCancel();
    return null;
  }

  return (
    <Suspense
      fallback={
        <div className="flex h-full items-center justify-center">
          <Loader2 className="animate-spin text-primary" />
        </div>
      }
    >
      <CollectionForm initialValues={activeCollection} onSave={handleSave} onCancel={onCancel} />
    </Suspense>
  );
};
