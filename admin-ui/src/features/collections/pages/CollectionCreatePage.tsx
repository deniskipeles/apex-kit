import React from 'react';
import { CollectionForm } from '../components/CollectionCreator';
import { collectionsService } from '../services/collectionsService';

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
    <CollectionForm 
      onSave={handleSave} 
      onCancel={onCancel} 
    />
  );
};
