import React, { Suspense } from 'react';
import { collectionsService } from '../services/collectionsService';
import { Loader2 } from 'lucide-react';

// Dynamic Import to enable code-splitting
const CollectionForm = React.lazy(() =>
  import('../components/CollectionCreator').then((module) => ({ default: module.CollectionForm }))
);

interface CollectionCreatePageProps {
  onCancel: () => void;
  onSuccess: () => void;
}

export const CollectionCreatePage = ({ onCancel, onSuccess }: CollectionCreatePageProps) => {
  const handleSave = async (data: any) => {
    await collectionsService.create(data);
    onSuccess();
  };

  return (
    <Suspense
      fallback={
        <div className="flex h-full items-center justify-center">
          <Loader2 className="animate-spin text-primary" />
        </div>
      }
    >
      <CollectionForm onSave={handleSave} onCancel={onCancel} />
    </Suspense>
  );
};
