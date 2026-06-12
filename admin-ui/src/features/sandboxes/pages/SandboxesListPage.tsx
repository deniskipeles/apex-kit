import React, { useState, useEffect, useCallback } from 'react';
import { Plus, Trash2, Play, BoxIcon, RefreshCw, Calendar, AlertTriangle } from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { CreateSandboxModal } from '../components/CreateSandboxModal';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { architectService } from '../../ai/services/architectService';
import { apiClient } from '../../../lib/apiClient';
import { useToast } from '../../../components/feedback/Toast';
import { SandboxMetadata } from '../../../types';

interface SandboxesListPageProps {
  onNavigate: (view: string) => void;
}

export const SandboxesListPage = ({ onNavigate }: SandboxesListPageProps) => {
  const { toast } = useToast();

  // Lists & Loaders State
  const [sandboxes, setSandboxes] = useState<SandboxMetadata[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  // Modals Toggle State
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [sandboxToDelete, setSandboxToDelete] = useState<SandboxMetadata | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const fetchSandboxes = useCallback(async () => {
    setIsLoading(true);
    try {
      const list = await architectService.listSessions();
      setSandboxes(list);
    } catch (e) {
      toast('Failed to load active sandboxes', 'error');
    } finally {
      setIsLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    fetchSandboxes();
  }, [fetchSandboxes]);

  const handleLaunch = (sandboxId: string) => {
    // Redirect scope into the sandbox environment
    onNavigate(`sandbox__${sandboxId}__dashboard`);
    toast('Switched context to sandbox environment', 'success');
  };

  const handleDelete = async () => {
    if (!sandboxToDelete) return;
    setIsDeleting(true);
    try {
      await apiClient.root.deleteSandbox(sandboxToDelete.id);
      toast(
        `Sandbox "${sandboxToDelete.name || sandboxToDelete.id}" successfully deleted`,
        'success'
      );
      setSandboxToDelete(null);
      fetchSandboxes();
    } catch (e: any) {
      toast(e.message || 'Failed to delete sandbox', 'error');
    } finally {
      setIsDeleting(false);
    }
  };

  const columns = [
    {
      field: 'name',
      headerName: 'Sandbox Details',
      renderCell: (s: SandboxMetadata) => (
        <div className="flex items-center gap-3 py-1">
          <div className="h-9 w-9 rounded-lg bg-amber-500/10 text-amber-500 border border-amber-500/20 flex items-center justify-center shrink-0">
            <BoxIcon className="h-5 w-5" />
          </div>
          <div className="flex flex-col min-w-0">
            <span
              className="font-bold text-foreground truncate max-w-[200px]"
              title={s.name || s.id}
            >
              {s.name || 'Unnamed Sandbox'}
            </span>
            <span className="text-[10px] text-muted-foreground font-mono">ID: {s.id}</span>
          </div>
        </div>
      ),
    },
    {
      field: 'status',
      headerName: 'Status',
      width: '120px',
      renderCell: (s: SandboxMetadata) => {
        const isActive = s.status === 'active';
        return (
          <Badge variant={isActive ? 'success' : 'secondary'} className="capitalize text-[10px]">
            {s.status}
          </Badge>
        );
      },
    },
    {
      field: 'expires_at',
      headerName: 'Expires',
      width: '180px',
      renderCell: (s: SandboxMetadata) => (
        <span className="text-xs text-muted-foreground font-mono flex items-center gap-1.5">
          <Calendar className="h-3.5 w-3.5 opacity-60" />
          {s.expires_at ? new Date(s.expires_at).toLocaleDateString() : 'Never'}
        </span>
      ),
    },
    {
      field: 'actions',
      headerName: '',
      width: '100px',
      align: 'right' as const,
      renderCell: (s: SandboxMetadata) => (
        <div className="flex justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-emerald-500 hover:bg-emerald-500/10"
            onClick={(e: any) => {
              e.stopPropagation();
              handleLaunch(s.id);
            }}
            title="Launch Sandbox"
          >
            <Play className="h-4 w-4" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
            onClick={(e: any) => {
              e.stopPropagation();
              setSandboxToDelete(s);
            }}
            title="Delete Sandbox"
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6 max-w-7xl mx-auto pb-20">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h2 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <BoxIcon className="h-8 w-8 text-amber-500" /> Ephemeral Sandboxes
          </h2>
          <p className="text-muted-foreground text-sm md:text-base">
            Isolate and prototype your logic securely. Merge with Production when ready.
          </p>
        </div>
        <Button onClick={() => setIsCreateOpen(true)} className="shadow-lg">
          <Plus className="mr-2 h-4 w-4" /> New Sandbox
        </Button>
      </div>

      {/* Toolbar */}
      <div className="flex items-center justify-between border-t border-border/50 pt-4">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <BoxIcon className="h-4 w-4" /> Total active sessions:{' '}
          <span className="font-bold text-foreground">{sandboxes.length}</span>
        </div>
        <Button variant="ghost" size="sm" onClick={fetchSandboxes} title="Refresh List">
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      {/* Main Table */}
      <div className="rounded-xl border border-border bg-card/50 backdrop-blur-sm overflow-hidden shadow-sm flex flex-col min-h-[300px]">
        <div className="flex-1">
          <DataGrid
            data={sandboxes}
            columns={columns}
            keyField="id"
            isLoading={isLoading}
            onRowClick={(row) => handleLaunch(row.id)}
          />
        </div>
      </div>

      {/* Integrated Advanced Creation Form */}
      <CreateSandboxModal
        isOpen={isCreateOpen}
        onClose={() => setIsCreateOpen(false)}
        onSuccess={fetchSandboxes}
      />

      {/* Delete Confirmation */}
      <ConfirmDialog
        isOpen={!!sandboxToDelete}
        title="Destroy Sandbox Environment?"
        description={`Are you sure you want to delete "${sandboxToDelete?.name || sandboxToDelete?.id}"? This will permanently wipe all local database records, custom scripts, and page assets.`}
        onConfirm={handleDelete}
        onCancel={() => setSandboxToDelete(null)}
        isLoading={isDeleting}
        variant="destructive"
        confirmText="Destroy Environment"
      />
    </div>
  );
};
