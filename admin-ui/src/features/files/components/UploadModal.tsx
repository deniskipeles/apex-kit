
import React, { useState } from 'react';
import { createPortal } from 'react-dom';
import { Upload, X, FileText, CheckCircle } from 'lucide-react';
import { Button } from '../../../components/ui/Elements';
import { FilePicker } from '../../../components/media/FilePicker';
import { formatFileSize } from '../../../lib/formatters';
import { useToast } from '../../../components/feedback/Toast';
import { filesService } from '../services/filesService';

interface UploadModalProps {
  isOpen: boolean;
  onClose: () => void;
  onUploadComplete: () => void;
}

export const UploadModal = ({ isOpen, onClose, onUploadComplete }: UploadModalProps) => {
  const [selectedFiles, setSelectedFiles] = useState<File[]>([]);
  const [uploading, setUploading] = useState(false);
  const [progress, setProgress] = useState<Record<string, number>>({});
  const { toast } = useToast();

  const handleUpload = async () => {
    if (selectedFiles.length === 0) return;
    
    setUploading(true);
    let successCount = 0;

    // Simulate individual uploads
    for (const file of selectedFiles) {
        try {
            setProgress(prev => ({ ...prev, [file.name]: 10 }));
            // Simulate progress
            const interval = setInterval(() => {
                setProgress(prev => ({ 
                    ...prev, 
                    [file.name]: Math.min((prev[file.name] || 0) + 10, 90) 
                }));
            }, 100);

            await filesService.upload(file);
            
            clearInterval(interval);
            setProgress(prev => ({ ...prev, [file.name]: 100 }));
            successCount++;
        } catch (e) {
            toast(`Failed to upload ${file.name}`, 'error');
        }
    }

    setUploading(false);
    toast(`Uploaded ${successCount} files successfully`, 'success');
    onUploadComplete();
    onClose();
    setSelectedFiles([]);
    setProgress({});
  };

  if (!isOpen) return null;

  return createPortal(
    <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 isolate">
       <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in" onClick={!uploading ? onClose : undefined} />
       <div className="relative w-full max-w-2xl bg-background rounded-xl border border-border shadow-2xl flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-200">
          <div className="flex items-center justify-between p-4 border-b">
             <h3 className="font-bold text-lg flex items-center gap-2"><Upload className="h-5 w-5" /> Upload Files</h3>
             {!uploading && <Button size="icon" variant="ghost" onClick={onClose}><X className="h-5 w-5" /></Button>}
          </div>
          
          <div className="flex-1 overflow-y-auto p-6 space-y-6">
             {!uploading ? (
                 <FilePicker onFilesSelected={setSelectedFiles} multiple />
             ) : (
                 <div className="space-y-3">
                     {selectedFiles.map((file, i) => (
                         <div key={i} className="flex items-center gap-3 p-3 rounded-lg border bg-card">
                             <FileText className="h-8 w-8 text-primary/50" />
                             <div className="flex-1 space-y-1">
                                 <div className="flex justify-between text-sm">
                                     <span className="font-medium">{file.name}</span>
                                     <span className="text-muted-foreground">{formatFileSize(file.size)}</span>
                                 </div>
                                 <div className="h-1.5 w-full bg-secondary rounded-full overflow-hidden">
                                     <div 
                                        className="h-full bg-primary transition-all duration-300" 
                                        style={{ width: `${progress[file.name] || 0}%` }} 
                                     />
                                 </div>
                             </div>
                             {progress[file.name] === 100 && <CheckCircle className="h-5 w-5 text-emerald-500" />}
                         </div>
                     ))}
                     <div className="text-center text-sm text-muted-foreground pt-4 animate-pulse">
                         Uploading files to storage...
                     </div>
                 </div>
             )}
          </div>

          {!uploading && (
             <div className="p-4 border-t bg-secondary/5 flex justify-end gap-3">
                 <Button variant="ghost" onClick={onClose}>Cancel</Button>
                 <Button onClick={handleUpload} disabled={selectedFiles.length === 0}>
                    <Upload className="mr-2 h-4 w-4" /> Start Upload
                 </Button>
             </div>
          )}
       </div>
    </div>,
    document.body
  );
};
