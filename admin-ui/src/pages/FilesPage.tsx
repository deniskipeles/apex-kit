import React, { useState, useEffect, useCallback } from 'react';
import {
  Search,
  LayoutGrid,
  List as ListIcon,
  Upload,
  Trash2,
  Download,
  X,
  Copy,
  FileIcon,
} from 'lucide-react';
import {
  Button,
  Input,
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Skeleton,
} from '../components/ui/Elements';
import { DataGrid } from '../components/data/DataGrid';
import { Pagination } from '../components/data/Pagination';
import { PreviewPanel } from '../components/preview/PreviewPanel';
import { FileThumbnail } from '../components/media/FileThumbnail';
import { FileCard } from '../features/files/components/FileCard';
import { UploadModal } from '../features/files/components/UploadModal';
import { filesService } from '../features/files/services/filesService';
import { StoredFile } from '../types';
import { formatFileSize } from '../lib/formatters';
import { useToast } from '../components/feedback/Toast';
import { ConfirmDialog } from '../components/feedback/ConfirmDialog';
import { usePagination } from '../hooks/usePagination';

export const FilesPage = () => {
  const [files, setFiles] = useState<StoredFile[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');
  const [search, setSearch] = useState('');
  const [selectedFile, setSelectedFile] = useState<StoredFile | null>(null);
  const [isUploadOpen, setIsUploadOpen] = useState(false);
  const [fileToDelete, setFileToDelete] = useState<StoredFile | null>(null);

  const { page, perPage, setPage, setPerPage } = usePagination(1, 24); // 24 fits nicely in grids
  const [totalItems, setTotalItems] = useState(0);

  const { toast } = useToast();

  const fetchFiles = useCallback(async () => {
    setIsLoading(true);
    try {
      const res = await filesService.list(page, perPage, search);
      setFiles(res.items);
      setTotalItems(res.totalItems);
    } catch (e) {
      toast('Failed to fetch files', 'error');
    } finally {
      setIsLoading(false);
    }
  }, [page, perPage, search, toast]);

  useEffect(() => {
    fetchFiles();
  }, [fetchFiles]);

  const handleDelete = async () => {
    if (!fileToDelete) return;
    try {
      await filesService.delete(fileToDelete.id);
      toast('File deleted successfully', 'success');
      setFileToDelete(null);
      if (selectedFile?.id === fileToDelete.id) setSelectedFile(null);
      fetchFiles();
    } catch (e) {
      toast('Failed to delete file', 'error');
    }
  };

  const copyUrl = (url: string) => {
    navigator.clipboard.writeText(url);
    toast('URL copied to clipboard', 'success');
  };

  const columns = [
    {
      field: 'preview',
      headerName: '',
      width: '50px',
      renderCell: (f: StoredFile) => (
        <div className="h-8 w-8 rounded overflow-hidden">
          <FileThumbnail url={f.url + '?thumb=100x100'} mimeType={f.mimeType} />
        </div>
      ),
    },
    {
      field: 'name',
      headerName: 'Name',
      renderCell: (f: StoredFile) => (
        <span className="font-medium truncate max-w-[200px] block" title={f.name}>
          {f.name}
        </span>
      ),
    },
    {
      field: 'mimeType',
      headerName: 'Type',
      width: '120px',
      renderCell: (f: StoredFile) => (
        <span className="text-xs text-muted-foreground bg-secondary/50 px-1.5 py-0.5 rounded">
          {f.mimeType.split('/')[1] || 'file'}
        </span>
      ),
    },
    {
      field: 'size',
      headerName: 'Size',
      width: '100px',
      renderCell: (f: StoredFile) => (
        <span className="text-sm text-muted-foreground">{formatFileSize(f.size)}</span>
      ),
    },
    {
      field: 'created',
      headerName: 'Uploaded',
      width: '150px',
      renderCell: (f: StoredFile) => (
        <span className="text-sm text-muted-foreground">
          {new Date(f.created).toLocaleDateString()}
        </span>
      ),
    },
    {
      field: 'actions',
      headerName: '',
      align: 'right' as const,
      width: '100px',
      renderCell: (f: StoredFile) => (
        <div className="flex justify-end gap-1">
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 text-muted-foreground"
            onClick={(e) => {
              e.stopPropagation();
              copyUrl(f.url);
            }}
            title="Copy URL"
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 text-muted-foreground hover:text-destructive"
            onClick={(e) => {
              e.stopPropagation();
              setFileToDelete(f);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6 h-[calc(100vh-140px)] flex flex-col">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 shrink-0">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">File Storage</h2>
          <p className="text-muted-foreground">Manage your uploaded assets.</p>
        </div>
        <div className="flex items-center gap-2">
          <div className="relative hidden sm:block w-64">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search files..."
              className="pl-9"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          <div className="flex items-center border rounded-md bg-card">
            <Button
              variant="ghost"
              size="icon"
              className={`rounded-none rounded-l-md ${viewMode === 'grid' ? 'bg-secondary text-foreground' : 'text-muted-foreground'}`}
              onClick={() => setViewMode('grid')}
            >
              <LayoutGrid className="h-4 w-4" />
            </Button>
            <div className="w-px h-6 bg-border"></div>
            <Button
              variant="ghost"
              size="icon"
              className={`rounded-none rounded-r-md ${viewMode === 'list' ? 'bg-secondary text-foreground' : 'text-muted-foreground'}`}
              onClick={() => setViewMode('list')}
            >
              <ListIcon className="h-4 w-4" />
            </Button>
          </div>
          <Button onClick={() => setIsUploadOpen(true)}>
            <Upload className="mr-2 h-4 w-4" /> Upload
          </Button>
        </div>
      </div>

      {/* Mobile Search */}
      <div className="sm:hidden relative shrink-0">
        <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="Search files..."
          className="pl-9"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      {/* Content Area */}
      <div className="flex-1 bg-card/30 rounded-xl border border-border/50 overflow-hidden flex flex-col relative">
        {isLoading ? (
          <div className="p-6 grid grid-cols-2 md:grid-cols-4 gap-4">
            {[1, 2, 3, 4, 5, 6, 7, 8].map((i) => (
              <Skeleton key={i} className="aspect-square rounded-xl" />
            ))}
          </div>
        ) : files.length === 0 ? (
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground p-8">
            <div className="p-4 rounded-full bg-secondary/50 mb-4">
              <FileIcon className="h-8 w-8 opacity-50" />
            </div>
            <h3 className="font-medium text-lg">No files found</h3>
            <p className="text-sm max-w-xs text-center mt-1">
              Upload files to get started or adjust your search terms.
            </p>
            <Button variant="outline" className="mt-4" onClick={() => setIsUploadOpen(true)}>
              Upload File
            </Button>
          </div>
        ) : (
          <>
            <div className="flex-1 overflow-y-auto p-4 custom-scrollbar">
              {viewMode === 'grid' ? (
                <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4">
                  {files.map((file) => (
                    <FileCard
                      key={file.id}
                      file={file}
                      onClick={() => setSelectedFile(file)}
                      onDelete={(e) => {
                        e.stopPropagation();
                        setFileToDelete(file);
                      }}
                    />
                  ))}
                </div>
              ) : (
                <div className="h-full">
                  <DataGrid
                    data={files}
                    columns={columns}
                    keyField="id"
                    onRowClick={(f) => setSelectedFile(f)}
                  />
                </div>
              )}
            </div>

            <div className="shrink-0 p-4 border-t bg-background flex justify-end">
              <Pagination
                page={page}
                totalPages={Math.ceil(totalItems / perPage) || 1}
                onPageChange={setPage}
              />
            </div>
          </>
        )}
      </div>

      {/* File Details Panel */}
      <PreviewPanel
        isOpen={!!selectedFile}
        onClose={() => setSelectedFile(null)}
        title={selectedFile?.name || 'File Details'}
        actions={
          selectedFile && (
            <>
              <Button
                variant="outline"
                className="flex-1"
                onClick={() => copyUrl(selectedFile.url)}
              >
                <Copy className="mr-2 h-4 w-4" /> Copy Link
              </Button>
              <Button
                variant="destructive"
                size="icon"
                onClick={() => setFileToDelete(selectedFile)}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </>
          )
        }
      >
        {selectedFile && (
          <div className="space-y-6">
            <div className="rounded-lg border border-border bg-secondary/5 overflow-hidden flex items-center justify-center min-h-[200px]">
              {selectedFile.mimeType.startsWith('image/') ? (
                <img
                  src={selectedFile.url}
                  alt={selectedFile.name}
                  className="max-w-full max-h-[400px] object-contain"
                />
              ) : (
                <div className="flex flex-col items-center gap-2 py-8">
                  <FileThumbnail
                    mimeType={selectedFile.mimeType}
                    className="h-20 w-20 rounded-none bg-transparent"
                  />
                  <p className="text-sm text-muted-foreground">No preview available</p>
                </div>
              )}
            </div>

            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="text-xs font-semibold text-muted-foreground uppercase">
                    Size
                  </label>
                  <p className="font-mono text-sm">{formatFileSize(selectedFile.size)}</p>
                </div>
                <div>
                  <label className="text-xs font-semibold text-muted-foreground uppercase">
                    Type
                  </label>
                  <p className="font-mono text-sm">{selectedFile.mimeType}</p>
                </div>
              </div>
              <div>
                <label className="text-xs font-semibold text-muted-foreground uppercase">
                  Uploaded
                </label>
                <p className="font-mono text-sm">
                  {new Date(selectedFile.created).toLocaleString()}
                </p>
              </div>
              <div>
                <label className="text-xs font-semibold text-muted-foreground uppercase">
                  Public URL
                </label>
                <div className="flex gap-2 mt-1">
                  <Input
                    readOnly
                    value={selectedFile.url}
                    className="font-mono text-xs h-8 bg-secondary/50"
                  />
                  <a href={selectedFile.url} target="_blank" rel="noreferrer">
                    <Button size="sm" variant="outline" className="h-8 w-8 p-0">
                      <Download className="h-3.5 w-3.5" />
                    </Button>
                  </a>
                </div>
              </div>
            </div>
          </div>
        )}
      </PreviewPanel>

      <UploadModal
        isOpen={isUploadOpen}
        onClose={() => setIsUploadOpen(false)}
        onUploadComplete={fetchFiles}
      />

      <ConfirmDialog
        isOpen={!!fileToDelete}
        title="Delete File"
        description={`Are you sure you want to delete "${fileToDelete?.name}"? This action cannot be undone.`}
        onConfirm={handleDelete}
        onCancel={() => setFileToDelete(null)}
        variant="destructive"
      />
    </div>
  );
};
