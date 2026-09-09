import React, { useState, useEffect, useRef } from 'react';
import { Layers, Upload, Trash2, Link as LinkIcon, RefreshCw, Cpu, BinaryIcon } from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Badge,
  Label,
  Input,
} from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { useToast } from '../../../components/feedback/Toast';
import { formatFileSize } from '../../../lib/formatters';
import { apiClient } from '@/src/lib/apiClient';

interface WasmModule {
  name: string;
  size: number;
  is_symlink: boolean;
  target: string | null;
}

export const WasmSettings = () => {
  const [modules, setModules] = useState<WasmModule[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isUploading, setIsUploading] = useState(false);
  const [uploadName, setUploadName] = useState('');

  const [isFetchOpen, setIsFetchOpen] = useState(false);
  const [fetchUrl, setFetchUrl] = useState('');
  const [fetchName, setFetchName] = useState('');
  const [isFetching, setIsFetching] = useState(false);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const { toast } = useToast();

  const loadModules = async () => {
    setIsLoading(true);
    try {
      const res = await apiClient.root.listWasm();
      setModules(res);
    } catch (e) {
      toast('Failed to load WASM cache', 'error');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadModules();
  }, []);

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    if (!file.name.endsWith('.wasm')) {
      toast('File must be a .wasm binary', 'error');
      e.target.value = '';
      return;
    }

    setIsUploading(true);
    toast('Validating & compiling WASM...', 'info');

    const nameToUse = uploadName.trim() || file.name;

    try {
      await apiClient.root.uploadWasm(nameToUse, file);
      toast(`Compiled & cached ${nameToUse} successfully`, 'success');
      setUploadName('');
      loadModules();
    } catch (e: any) {
      toast(e.message || 'WASM compilation failed. Check file integrity.', 'error');
    } finally {
      setIsUploading(false);
      e.target.value = '';
    }
  };

  const handleFetch = async () => {
    if (!fetchUrl.trim()) return;
    setIsFetching(true);
    toast('Downloading & compiling WASM...', 'info');

    try {
      await apiClient.root.fetchWasm(fetchUrl.trim(), fetchName.trim() || undefined);
      toast('WASM binary fetched, compiled, and cached successfully', 'success');
      setIsFetchOpen(false);
      setFetchUrl('');
      setFetchName('');
      loadModules();
    } catch (e: any) {
      toast(e.message || 'Failed to fetch or compile WASM binary.', 'error');
    } finally {
      setIsFetching(false);
    }
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`Delete ${name}? Scripts referencing this binary will fail.`)) return;
    try {
      await apiClient.root.deleteWasm(name);
      toast('Deleted successfully', 'success');
      loadModules();
    } catch (e) {
      toast('Failed to delete module', 'error');
    }
  };

  return (
    <div className="space-y-6">
      <Card className="border-indigo-500/20 bg-indigo-500/5">
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-indigo-500">
            <Cpu className="h-5 w-5" /> WebAssembly (WASI) Runtime Cache
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">
            WASM binaries uploaded here are validated, Ahead-of-Time (AOT) compiled to native
            machine code (`.cwasm`), and cached locally. You can execute them inside your Scripts
            using{' '}
            <code className="text-xs bg-background border px-1 rounded">
              $wasm.call("name.wasm", "func", [])
            </code>
            .
          </p>

          <div className="flex flex-col sm:flex-row gap-3 pt-2">
            <div className="flex-1 space-y-1">
              <Label>Custom Name (Optional)</Label>
              <Input
                placeholder="e.g. ffmpeg.wasm"
                value={uploadName}
                onChange={(e: any) => setUploadName(e.target.value)}
                disabled={isUploading}
              />
            </div>
            <div className="flex items-end">
              <input
                type="file"
                accept=".wasm"
                ref={fileInputRef}
                className="hidden"
                onChange={handleUpload}
              />
              <Button
                onClick={() => fileInputRef.current?.click()}
                isLoading={isUploading}
                className="w-full sm:w-auto bg-indigo-600 hover:bg-indigo-700 text-white rounded-r-none"
              >
                <Upload className="mr-2 h-4 w-4" /> Upload Local
              </Button>
              <div className="w-px bg-indigo-500/50 h-9"></div>
              <Button
                onClick={() => setIsFetchOpen(true)}
                className="w-full sm:w-auto bg-indigo-600 hover:bg-indigo-700 text-white rounded-l-none border-l-0"
              >
                <LinkIcon className="mr-2 h-4 w-4" /> Download via Link
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between pb-2">
          <CardTitle className="text-base flex items-center gap-2">
            <Layers className="h-4 w-4 text-muted-foreground" /> Cached Modules ({modules.length})
          </CardTitle>
          <Button variant="ghost" size="sm" onClick={loadModules} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </CardHeader>
        <CardContent>
          <div className="rounded-md border border-border overflow-x-auto">
            <table className="w-full min-w-[540px] text-sm text-left">
              <thead className="bg-secondary/30 text-xs font-semibold text-muted-foreground">
                <tr>
                  <th className="px-4 py-3">File / Symlink</th>
                  <th className="px-4 py-3 w-40 sm:w-48">Target Hash</th>
                  <th className="px-4 py-3 w-24 text-right">Size</th>
                  <th className="px-4 py-3 w-12 text-right"></th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border bg-card">
                {modules.length === 0 ? (
                  <tr>
                    <td
                      colSpan={4}
                      className="p-8 text-center text-muted-foreground text-sm italic"
                    >
                      No cached WASM binaries found.
                    </td>
                  </tr>
                ) : (
                  modules.map((m) => (
                    <tr key={m.name} className="hover:bg-secondary/10 group transition-colors">
                      <td className="px-4 py-3 max-w-[200px] sm:max-w-[280px]">
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="shrink-0">
                            {m.is_symlink ? (
                              <LinkIcon className="h-4 w-4 text-blue-400" />
                            ) : m.name.endsWith('.cwasm') ? (
                              <Cpu className="h-4 w-4 text-emerald-500" />
                            ) : (
                              <BinaryIcon className="h-4 w-4 text-amber-500" />
                            )}
                          </span>
                          <span
                            title={m.name}
                            className={`font-mono font-medium truncate block ${
                              m.is_symlink ? 'text-primary' : 'text-muted-foreground'
                            }`}
                          >
                            {m.name}
                          </span>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-xs font-mono text-muted-foreground max-w-[160px] sm:max-w-[200px]">
                        <span title={m.target || '-'} className="truncate block">
                          {m.target || '-'}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-right text-xs font-mono text-muted-foreground whitespace-nowrap">
                        {formatFileSize(m.size)}
                      </td>
                      <td className="px-4 py-3 text-right">
                        <Button
                          size="icon"
                          variant="ghost"
                          className="h-7 w-7 text-muted-foreground hover:text-destructive opacity-80 group-hover:opacity-100 transition-opacity"
                          onClick={() => handleDelete(m.name)}
                          title="Delete"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>

      <Dialog
        isOpen={isFetchOpen}
        onClose={() => setIsFetchOpen(false)}
        title="Fetch WASM Module"
        size="sm"
      >
        <div className="space-y-4">
          <p className="text-sm text-muted-foreground">
            Download a WebAssembly binary directly from a URL (e.g., GitHub Releases, Unpkg, or a
            direct link).
          </p>
          <div className="space-y-2">
            <Label required>URL</Label>
            <Input
              value={fetchUrl}
              onChange={(e: any) => setFetchUrl(e.target.value)}
              placeholder="https://example.com/module.wasm"
            />
          </div>
          <div className="space-y-2">
            <Label>Custom File Name (Optional)</Label>
            <Input
              value={fetchName}
              onChange={(e: any) => setFetchName(e.target.value)}
              placeholder="e.g. module.wasm"
            />
          </div>
          <div className="flex justify-end gap-2 pt-2 border-t border-border mt-4">
            <Button variant="ghost" onClick={() => setIsFetchOpen(false)} disabled={isFetching}>
              Cancel
            </Button>
            <Button onClick={handleFetch} isLoading={isFetching} disabled={!fetchUrl}>
              Download & Compile
            </Button>
          </div>
        </div>
      </Dialog>
    </div>
  );
};
