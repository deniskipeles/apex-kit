
import React from 'react';
import { CollectionForm } from '../components/CollectionCreator';
import { useCollectionsStore } from '../../../store/useCollectionsStore';

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
      // Fallback if no collection selected (e.g. refresh), go back
      onCancel();
      return null;
  }

  return (
    <CollectionForm 
      initialValues={activeCollection}
      onSave={handleSave} 
      onCancel={onCancel} 
    />
  );
};