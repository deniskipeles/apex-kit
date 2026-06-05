import React, { useRef } from 'react';
import { Download, Upload } from 'lucide-react';
import { Button } from './ui/Elements';
import { useToast } from './feedback/Toast';

interface Props {
  onExport: () => Promise<void>;
  onImport: (file: File) => Promise<any>;
}

export const ImportExportToolbar = ({ onExport, onImport }: Props) => {
  const { toast } = useToast();
  const fileInput = useRef<HTMLInputElement>(null);

  const handleImportClick = () => fileInput.current?.click();

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    try {
      const res = await onImport(file);
      toast(`Imported: ${res.created} created, ${res.updated} updated`, 'success');
      if (res.errors?.length) toast('Some items failed to import', 'warning');
      setTimeout(() => window.location.reload(), 1000); // Simple refresh
    } catch (err: any) {
      toast(err.message, 'error');
    }

    e.target.value = ''; // Reset
  };

  return (
    <div className="flex gap-2">
      <input
        type="file"
        ref={fileInput}
        className="hidden"
        accept=".json"
        onChange={handleFileChange}
      />
      <Button variant="outline" size="sm" onClick={onExport}>
        <Download className="mr-2 h-3 w-3" /> Export
      </Button>
      <Button variant="outline" size="sm" onClick={handleImportClick}>
        <Upload className="mr-2 h-3 w-3" /> Import
      </Button>
    </div>
  );
};
