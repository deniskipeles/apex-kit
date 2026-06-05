import React, { useState, useEffect, useCallback } from 'react';
import {
  Plus,
  Server,
  Edit,
  Trash2,
  ShieldAlert,
  Check,
  X,
  Search,
  Database,
  Cpu,
  HardDrive,
  LayoutGrid,
  List as ListIcon,
  RefreshCw,
  ExternalLink,
  Sparkles,
  Ban,
  AlertTriangle,
} from 'lucide-react';
import {
  Button,
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Badge,
  Input,
  Select,
  Label,
  Skeleton,
} from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '../../../lib/apiClient';
import { Tenant } from '../../../types';
import { formatFileSize } from '../../../lib/formatters';

export const TenantsListPage = () => {
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');

  // Modals / Dialogs State
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [deletingTenant, setDeletingTenant] = useState<Tenant | null>(null);
  const [editingTenant, setEditingTenant] = useState<Tenant | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // Form State
  const [formData, setFormData] = useState({
    tenantId: '',
    name: '',
    tier: 'free',
    ownerId: '',
  });

  const { toast } = useToast();

  // Load Tenants
  const fetchTenants = useCallback(async () => {
    setLoading(true);
    try {
      const list = await apiClient.root.listTenants();
      setTenants(list);
    } catch (e: any) {
      toast(e.message || 'Failed to load tenants list', 'error');
    } finally {
      setLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    fetchTenants();
  }, [fetchTenants]);

  // Handle Create Submit
  const handleCreateSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.tenantId.trim()) return;

    // Validate ID format (Subdomain safe)
    const idRegex = /^[a-z0-9-]+$/;
    if (!idRegex.test(formData.tenantId)) {
      toast('Tenant ID must be lowercase alphanumeric and dashes only.', 'error');
      return;
    }

    setSubmitting(true);
    try {
      // 1. Provision the tenant container
      await apiClient.root.createTenant(formData.tenantId);

      // 2. Set additional display parameters
      await apiClient.root.updateTenant(formData.tenantId, {
        name: formData.name || undefined,
        tier: formData.tier,
        owner_id: formData.ownerId ? Number(formData.ownerId) : undefined,
      });

      toast(`Tenant '${formData.tenantId}' successfully created.`, 'success');
      setIsCreateOpen(false);
      resetForm();
      fetchTenants();
    } catch (err: any) {
      toast(err.message || 'Failed to create tenant', 'error');
    } finally {
      setSubmitting(false);
    }
  };

  // Handle Edit Submit
  const handleEditSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingTenant) return;

    setSubmitting(true);
    try {
      await apiClient.root.updateTenant(editingTenant.id, {
        name: formData.name || undefined,
        tier: formData.tier,
      });
      toast(`Tenant metadata updated successfully.`, 'success');
      setIsEditOpen(false);
      setEditingTenant(null);
      resetForm();
      fetchTenants();
    } catch (err: any) {
      toast(err.message || 'Failed to update tenant metadata', 'error');
    } finally {
      setSubmitting(false);
    }
  };

  // Handle Status Update
  const handleStatusChange = async (
    id: string,
    nextStatus: 'active' | 'suspended' | 'archived'
  ) => {
    try {
      await apiClient.root.updateStatus(id, nextStatus);
      toast(`Tenant status changed to ${nextStatus}`, 'success');
      fetchTenants();
    } catch (err: any) {
      toast(err.message || 'Failed to update tenant status', 'error');
    }
  };

  // Handle Hard Delete
  const handleDeleteConfirm = async () => {
    if (!deletingTenant) return;
    setSubmitting(true);
    try {
      await apiClient.root.deleteTenant(deletingTenant.id);
      toast(`Tenant '${deletingTenant.id}' and its resources have been deleted.`, 'success');
      setDeletingTenant(null);
      fetchTenants();
    } catch (err: any) {
      toast(err.message || 'Failed to delete tenant', 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const openEditModal = (tenant: Tenant) => {
    setEditingTenant(tenant);
    setFormData({
      tenantId: tenant.id,
      name: tenant.name || '',
      tier: tenant.tier,
      ownerId: '', // Owner IDs are non-updatable in this view for safety
    });
    setIsEditOpen(true);
  };

  const resetForm = () => {
    setFormData({
      tenantId: '',
      name: '',
      tier: 'free',
      ownerId: '',
    });
  };

  const filteredTenants = tenants.filter(
    (t) =>
      t.id.toLowerCase().includes(search.toLowerCase()) ||
      (t.name && t.name.toLowerCase().includes(search.toLowerCase()))
  );

  return (
    <div className="space-y-6 max-w-7xl mx-auto pb-20">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div>
          <h2 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <Server className="h-8 w-8 text-primary" /> Multi-Tenancy
          </h2>
          <p className="text-muted-foreground text-sm md:text-base">
            Provision, monitor, and regulate isolated tenant container environments.
          </p>
        </div>
        <div className="flex items-center gap-3">
          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search tenants..."
              className="pl-9 w-full md:w-64 bg-background/50"
              value={search}
              onChange={(e: any) => setSearch(e.target.value)}
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
          <Button
            onClick={() => {
              resetForm();
              setIsCreateOpen(true);
            }}
            className="shadow-lg"
          >
            <Plus className="mr-2 h-4 w-4" /> New Tenant
          </Button>
        </div>
      </div>

      {/* Grid view of Tenants */}
      {loading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {[1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-64 w-full rounded-xl" />
          ))}
        </div>
      ) : filteredTenants.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-center border-2 border-dashed border-border rounded-xl bg-secondary/5">
          <Server className="h-10 w-10 text-muted-foreground/50 mb-4" />
          <h3 className="text-xl font-semibold mb-2">No Tenants Found</h3>
          <p className="text-muted-foreground max-w-md">
            {search
              ? `No matches for "${search}"`
              : 'Provision your first client context to begin hosting.'}
          </p>
        </div>
      ) : viewMode === 'grid' ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {filteredTenants.map((tenant) => (
            <Card
              key={tenant.id}
              className="relative overflow-hidden transition-all duration-300 hover:shadow-xl border-border hover:border-primary/40 flex flex-col justify-between"
            >
              <div className="p-6 space-y-4">
                {/* ID Header */}
                <div className="flex items-start justify-between">
                  <div>
                    <h3
                      className="text-xl font-bold text-foreground truncate max-w-[200px]"
                      title={tenant.name || tenant.id}
                    >
                      {tenant.name || tenant.id}
                    </h3>
                    <span className="font-mono text-xs text-muted-foreground">ID: {tenant.id}</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <Badge
                      variant={tenant.tier === 'pro' ? 'primary' : 'secondary'}
                      className="capitalize"
                    >
                      {tenant.tier}
                    </Badge>
                    <Badge
                      variant={tenant.status === 'active' ? 'success' : 'destructive'}
                      className="capitalize"
                    >
                      {tenant.status}
                    </Badge>
                  </div>
                </div>

                {/* Meter Limits Progress Bars */}
                <div className="space-y-3 pt-2">
                  <div className="space-y-1">
                    <div className="flex justify-between text-xs font-semibold text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <HardDrive className="h-3.5 w-3.5" /> Storage
                      </span>
                      <span>
                        {tenant.stats.storage_mb.toFixed(1)} / {tenant.stats.max_storage_mb} MB
                      </span>
                    </div>
                    <div className="w-full bg-secondary/30 h-1.5 rounded-full overflow-hidden border">
                      <div
                        className={`h-full ${tenant.stats.storage_mb / tenant.stats.max_storage_mb > 0.85 ? 'bg-destructive' : 'bg-primary'}`}
                        style={{
                          width: `${Math.min(100, (tenant.stats.storage_mb / tenant.stats.max_storage_mb) * 100)}%`,
                        }}
                      ></div>
                    </div>
                  </div>

                  <div className="space-y-1">
                    <div className="flex justify-between text-xs font-semibold text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <Database className="h-3.5 w-3.5" /> Vectors
                      </span>
                      <span>
                        {tenant.stats.vector_count} / {tenant.stats.max_vectors}
                      </span>
                    </div>
                    <div className="w-full bg-secondary/30 h-1.5 rounded-full overflow-hidden border">
                      <div
                        className="bg-purple-500 h-full"
                        style={{
                          width: `${Math.min(100, (tenant.stats.vector_count / tenant.stats.max_vectors) * 100)}%`,
                        }}
                      ></div>
                    </div>
                  </div>

                  <div className="space-y-1">
                    <div className="flex justify-between text-xs font-semibold text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <Sparkles className="h-3.5 w-3.5" /> AI Requests
                      </span>
                      <span>
                        {tenant.stats.ai_requests} / {tenant.stats.max_ai_requests}
                      </span>
                    </div>
                    <div className="w-full bg-secondary/30 h-1.5 rounded-full overflow-hidden border">
                      <div
                        className="bg-indigo-500 h-full"
                        style={{
                          width: `${Math.min(100, (tenant.stats.ai_requests / tenant.stats.max_ai_requests) * 100)}%`,
                        }}
                      ></div>
                    </div>
                  </div>
                </div>
              </div>

              {/* Action Bar */}
              <div className="p-4 border-t border-border bg-secondary/10 flex items-center justify-between mt-auto">
                <div className="flex items-center gap-1">
                  <Select
                    value={tenant.status}
                    onChange={(e: any) => handleStatusChange(tenant.id, e.target.value as any)}
                    className="h-8 text-xs py-0 w-28 bg-background border-none shadow-none"
                  >
                    <option value="active">Active</option>
                    <option value="suspended">Suspended</option>
                    <option value="archived">Archived</option>
                  </Select>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-muted-foreground"
                    onClick={() => openEditModal(tenant)}
                  >
                    <Edit className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8 text-muted-foreground hover:text-destructive"
                    onClick={() => setDeletingTenant(tenant)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>

                <a href={`/_dashboard/tenant/${tenant.id}`} target="_blank" rel="noreferrer">
                  <Button size="sm" variant="outline" className="h-8 text-xs gap-1.5">
                    Launch <ExternalLink className="h-3.5 w-3.5" />
                  </Button>
                </a>
              </div>
            </Card>
          ))}
        </div>
      ) : (
        /* List View (Table Grid) */
        <div className="rounded-xl border border-border bg-card overflow-hidden">
          <table className="w-full text-sm text-left">
            <thead className="bg-secondary/30 text-xs font-semibold uppercase text-muted-foreground">
              <tr>
                <th className="px-4 py-3">Tenant ID / Name</th>
                <th className="px-4 py-3">Tier</th>
                <th className="px-4 py-3">Status</th>
                <th className="px-4 py-3 text-right">Disk Usage</th>
                <th className="px-4 py-3 text-right">Created</th>
                <th className="px-4 py-3 w-[150px]"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {filteredTenants.map((tenant) => (
                <tr key={tenant.id} className="hover:bg-secondary/5 transition-colors group">
                  <td className="px-4 py-3">
                    <div className="font-semibold text-foreground">{tenant.name || tenant.id}</div>
                    <div className="text-xs text-muted-foreground font-mono">{tenant.id}</div>
                  </td>
                  <td className="px-4 py-3 capitalize">
                    <Badge variant={tenant.tier === 'pro' ? 'primary' : 'secondary'}>
                      {tenant.tier}
                    </Badge>
                  </td>
                  <td className="px-4 py-3 capitalize">
                    <Badge variant={tenant.status === 'active' ? 'success' : 'destructive'}>
                      {tenant.status}
                    </Badge>
                  </td>
                  <td className="px-4 py-3 text-right font-mono text-xs">
                    {tenant.stats.storage_mb.toFixed(1)} MB
                  </td>
                  <td className="px-4 py-3 text-right text-xs text-muted-foreground">
                    {new Date(tenant.created_at).toLocaleDateString()}
                  </td>
                  <td className="px-4 py-3 text-right">
                    <div className="flex justify-end gap-1 opacity-0 group-hover:opacity-100 transition-all">
                      <a href={`/_dashboard/tenant/${tenant.id}`} target="_blank" rel="noreferrer">
                        <Button
                          size="icon"
                          variant="ghost"
                          className="h-8 w-8 text-muted-foreground"
                        >
                          <ExternalLink className="h-4 w-4" />
                        </Button>
                      </a>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-8 w-8 text-muted-foreground"
                        onClick={() => openEditModal(tenant)}
                      >
                        <Edit className="h-4 w-4" />
                      </Button>
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-8 w-8 text-muted-foreground hover:text-destructive"
                        onClick={() => setDeletingTenant(tenant)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* CREATE MODAL */}
      <Dialog
        isOpen={isCreateOpen}
        onClose={() => setIsCreateOpen(false)}
        title="Provision New Tenant"
        size="sm"
      >
        <form onSubmit={handleCreateSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label required>Tenant ID (URL Subdomain)</Label>
            <Input
              placeholder="e.g. customer-1"
              value={formData.tenantId}
              onChange={(e: any) =>
                setFormData({ ...formData, tenantId: e.target.value.toLowerCase() })
              }
              className="font-mono"
              required
            />
            <p className="text-[9px] text-muted-foreground">
              Lowercase, alphanumeric, and dashes only. Becomes url subdomain.
            </p>
          </div>

          <div className="space-y-2">
            <Label>Display Name</Label>
            <Input
              placeholder="e.g. Acme Corp"
              value={formData.name}
              onChange={(e: any) => setFormData({ ...formData, name: e.target.value })}
            />
          </div>

          <div className="space-y-2">
            <Label>Billing Tier</Label>
            <Select
              value={formData.tier}
              onChange={(e: any) => setFormData({ ...formData, tier: e.target.value })}
            >
              <option value="free">Free Tier (Standard Limits)</option>
              <option value="pro">Pro Tier (Enterprise Limits)</option>
            </Select>
          </div>

          <div className="space-y-2">
            <Label>Owner User ID (Optional)</Label>
            <Input
              type="number"
              placeholder="e.g. 1"
              value={formData.ownerId}
              onChange={(e: any) => setFormData({ ...formData, ownerId: e.target.value })}
            />
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-border mt-4">
            <Button type="button" variant="ghost" onClick={() => setIsCreateOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" isLoading={submitting}>
              Provision
            </Button>
          </div>
        </form>
      </Dialog>

      {/* EDIT MODAL */}
      <Dialog
        isOpen={isEditOpen}
        onClose={() => setIsEditOpen(false)}
        title={`Configure Metadata: ${editingTenant?.id}`}
        size="sm"
      >
        <form onSubmit={handleEditSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label>Display Name</Label>
            <Input
              placeholder="e.g. Acme Corp"
              value={formData.name}
              onChange={(e: any) => setFormData({ ...formData, name: e.target.value })}
            />
          </div>

          <div className="space-y-2">
            <Label>Billing Tier</Label>
            <Select
              value={formData.tier}
              onChange={(e: any) => setFormData({ ...formData, tier: e.target.value })}
            >
              <option value="free">Free Tier (Standard Limits)</option>
              <option value="pro">Pro Tier (Enterprise Limits)</option>
            </Select>
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-border mt-4">
            <Button type="button" variant="ghost" onClick={() => setIsEditOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" isLoading={submitting}>
              Save Changes
            </Button>
          </div>
        </form>
      </Dialog>

      {/* DELETE CONFIRMATION */}
      <ConfirmDialog
        isOpen={!!deletingTenant}
        title="Destroy Tenant Instance?"
        description={`This will permanently delete the tenant '${deletingTenant?.id}', destroying its filesystem structure, SQLite databases, and all stored assets. This action is irreversible.`}
        confirmText="Destroy Instance"
        variant="destructive"
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeletingTenant(null)}
        isLoading={submitting}
      />
    </div>
  );
};
