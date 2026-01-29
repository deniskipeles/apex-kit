import React, { useState, useEffect } from 'react';
import { Plus, Search, Building, ArrowRight, ExternalLink, Server, Database, Trash2, Power, HardDrive } from 'lucide-react';
import { Button, Input, Card, CardHeader, CardTitle, CardContent, Badge } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { Label } from '../../../components/form/FormPrimitives';
import { apiClient } from '../../../lib/apiClient';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '@/src/components/feedback/ConfirmDialog';
import { Tenant } from '@/src/types';

const CreateTenantDialog = ({ isOpen, onClose, onSuccess }: { isOpen: boolean; onClose: () => void; onSuccess: () => void }) => {
    const [tenantId, setTenantId] = useState('');
    const [isLoading, setIsLoading] = useState(false);
    const { toast } = useToast();

    const handleSubmit = async () => {
        if (!tenantId.trim()) return;
        setIsLoading(true);
        try {
            await apiClient.root.createTenant(tenantId);
            toast('Tenant created successfully', 'success');
            onSuccess();
            onClose();
            setTenantId('');
        } catch (e: any) {
            toast(e.message || 'Failed to create tenant', 'error');
        } finally {
            setIsLoading(false);
        }
    };

    return (
        <Dialog isOpen={isOpen} onClose={onClose} title="Provision New Tenant" size="sm">
            <div className="space-y-4">
                <div className="p-3 bg-secondary/10 rounded-md border border-border text-xs text-muted-foreground">
                    This will create a dedicated, isolated database and storage environment.
                </div>
                <div className="space-y-2">
                    <Label required>Tenant ID (Subdomain)</Label>
                    <Input
                        value={tenantId}
                        onChange={(e: any) => setTenantId(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
                        placeholder="e.g. customer-acme"
                        autoFocus
                    />
                    <p className="text-[10px] text-muted-foreground">Lowercase alphanumeric and hyphens only.</p>
                </div>
                <div className="flex justify-end gap-2 pt-2">
                    <Button variant="ghost" onClick={onClose}>Cancel</Button>
                    <Button onClick={handleSubmit} isLoading={isLoading} disabled={!tenantId}>Provision</Button>
                </div>
            </div>
        </Dialog>
    );
};

export const TenantsListPage = () => {
    // Change state to array of Tenant objects
    const [tenants, setTenants] = useState<Tenant[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [isCreateOpen, setIsCreateOpen] = useState(false);
    
    // Actions State
    const [deleteId, setDeleteId] = useState<string | null>(null);
    const [isDeleting, setIsDeleting] = useState(false);
    
    const { toast } = useToast();

    // Helper: Deterministic color
    const getColor = (id: string) => {
        const colors = ['bg-blue-500', 'bg-purple-500', 'bg-emerald-500', 'bg-orange-500', 'bg-pink-500'];
        let hash = 0;
        for (let i = 0; i < id.length; i++) hash = id.charCodeAt(i) + ((hash << 5) - hash);
        return colors[Math.abs(hash) % colors.length];
    };

    const fetchTenants = async () => {
        setLoading(true);
        try {
            const list = await apiClient.root.listTenants();
            setTenants(list);
        } catch (e) {
            toast("Failed to load tenants", "error");
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        fetchTenants();
    }, []);

    const handleManage = (id: string) => {
        window.location.href = `/_dashboard/tenant/${id}/dashboard`;
    };

    const handlePublicUrl = (id: string) => {
        const url = `${window.location.origin}/tenant/${id}`;
        window.open(url, '_blank');
    };

    const handleDelete = async () => {
        if (!deleteId) return;
        setIsDeleting(true);
        try {
            await apiClient.root.deleteTenant(deleteId);
            toast(`Tenant ${deleteId} deleted`, 'success');
            setTenants(prev => prev.filter(t => t.id !== deleteId));
        } catch (e) {
            toast('Failed to delete tenant', 'error');
        } finally {
            setIsDeleting(false);
            setDeleteId(null);
        }
    };
    
    const handleToggleStatus = async (id: string, currentStatus: string) => {
        // Calculate new status
        const newStatus = currentStatus === 'active' ? 'suspended' : 'active';
        
        try {
            await apiClient.root.updateStatus(id, newStatus as any); 
            
            // Optimistic Update
            setTenants(prev => prev.map(t => 
                t.id === id ? { ...t, status: newStatus } : t
            ));
            
            toast(`Tenant ${id} is now ${newStatus}`, 'success');
        } catch(e) {
             toast('Failed to update status', 'error');
        }
    };

    const filtered = tenants.filter(t => t.id.toLowerCase().includes(search.toLowerCase()));

    return (
        <div className="space-y-8 max-w-7xl mx-auto pb-20">
            {/* Header */}
            <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
                <div className="space-y-1">
                    <h1 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
                        <Server className="h-8 w-8 text-primary" /> Multi-Tenancy
                    </h1>
                    <p className="text-muted-foreground text-sm md:text-base max-w-2xl">
                        Manage isolated environments. Each tenant has its own Database, File Storage, and Search Index.
                    </p>
                </div>
                <div className="flex gap-2">
                    <div className="relative">
                        <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                        <Input 
                            placeholder="Search tenants..." 
                            className="pl-9 w-64 bg-background/50" 
                            value={search}
                            onChange={(e: any) => setSearch(e.target.value)}
                        />
                    </div>
                    <Button onClick={() => setIsCreateOpen(true)} className="shadow-lg">
                        <Plus className="mr-2 h-4 w-4" /> Provision Tenant
                    </Button>
                </div>
            </div>

            {/* Grid */}
            {loading ? (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                    {[1, 2, 3].map(i => (
                        <div key={i} className="h-48 rounded-xl bg-secondary/20 animate-pulse border border-border/50" />
                    ))}
                </div>
            ) : filtered.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-20 text-center border-2 border-dashed border-border rounded-xl bg-secondary/5">
                    <div className="h-20 w-20 bg-secondary/30 rounded-full flex items-center justify-center mb-4">
                        <Building className="h-10 w-10 text-muted-foreground/50" />
                    </div>
                    <h3 className="text-xl font-semibold mb-2">No Tenants Found</h3>
                    <p className="text-muted-foreground max-w-md mb-6">
                        {search ? "No matches for your search." : "You haven't provisioned any tenants yet."}
                    </p>
                    <Button variant="outline" onClick={() => { setSearch(''); setIsCreateOpen(true); }}>
                        Create your first Tenant
                    </Button>
                </div>
            ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                    {filtered.map(tenant => (
                        <Card key={tenant.id} className={`group relative overflow-hidden transition-all duration-300 hover:shadow-xl hover:border-primary/50 hover:-translate-y-1 bg-card/50 backdrop-blur-sm ${tenant.status === 'suspended' ? 'opacity-75 grayscale' : ''}`}>
                            {/* Decorative Background Gradient */}
                            <div className={`absolute top-0 inset-x-0 h-1 ${getColor(tenant.id)} opacity-80`} />
                            
                            <CardHeader className="pb-2">
                                <div className="flex justify-between items-start">
                                    <div className="flex items-center gap-3">
                                        <div className={`h-10 w-10 rounded-lg ${getColor(tenant.id)} bg-opacity-10 flex items-center justify-center text-white shadow-inner font-bold text-lg uppercase`}>
                                            {tenant.id.substring(0, 2)}
                                        </div>
                                        <div>
                                            <CardTitle className="text-lg truncate max-w-[150px]" title={tenant.id}>{tenant.name || tenant.id}</CardTitle>
                                            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mt-1">
                                                <div className={`h-1.5 w-1.5 rounded-full ${tenant.status === 'active' ? 'bg-emerald-500 animate-pulse' : 'bg-red-500'}`} />
                                                <span className="capitalize">{tenant.status}</span>
                                                <span className="text-muted-foreground/30">•</span>
                                                <Badge variant="outline" className="text-[9px] py-0 h-4 uppercase">{tenant.tier}</Badge>
                                            </div>
                                        </div>
                                    </div>
                                    
                                    {/* Action Menu */}
                                    <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                                        <Button variant="ghost" size="icon" className="h-8 w-8 text-muted-foreground hover:text-foreground" onClick={() => handlePublicUrl(tenant.id)} title="Public API">
                                            <ExternalLink className="h-4 w-4" />
                                        </Button>
                                        <Button variant="ghost" size="icon" className="h-8 w-8 text-muted-foreground hover:text-destructive" onClick={() => setDeleteId(tenant.id)} title="Delete">
                                            <Trash2 className="h-4 w-4" />
                                        </Button>
                                    </div>
                                </div>
                            </CardHeader>
                            
                            <CardContent>
                                <div className="grid grid-cols-2 gap-2 mt-2 mb-4">
                                    <div className="bg-secondary/30 p-2 rounded border border-border/50 flex flex-col gap-1">
                                        <span className="text-[10px] text-muted-foreground uppercase font-bold flex items-center gap-1">
                                            <HardDrive className="h-3 w-3" /> Storage
                                        </span>
                                        <div className="text-xs font-mono">
                                            {tenant.stats.storage_mb.toFixed(1)} <span className="text-muted-foreground">/ {tenant.stats.max_storage_mb} MB</span>
                                        </div>
                                        <div className="h-1 w-full bg-secondary rounded-full overflow-hidden">
                                            <div 
                                                className="h-full bg-blue-500/50" 
                                                style={{ width: `${Math.min((tenant.stats.storage_mb / tenant.stats.max_storage_mb) * 100, 100)}%` }} 
                                            />
                                        </div>
                                    </div>
                                    <div className="bg-secondary/30 p-2 rounded border border-border/50 flex flex-col gap-1">
                                        <span className="text-[10px] text-muted-foreground uppercase font-bold flex items-center gap-1">
                                            <Database className="h-3 w-3" /> Records
                                        </span>
                                        <span className="text-xs font-mono">
                                            {/* Note: This assumes vector_count roughly correlates or use total_records if added to stats */}
                                            {tenant.stats.vector_count} <span className="text-muted-foreground">vectors</span>
                                        </span>
                                    </div>
                                </div>
                                
                                <div className="flex gap-2">
                                     <Button 
                                        variant="outline" 
                                        size="icon" 
                                        className={`shrink-0 ${tenant.status === 'active' ? 'text-emerald-500 hover:text-red-500 hover:border-red-500/50' : 'text-red-500 hover:text-emerald-500 hover:border-emerald-500/50'}`}
                                        onClick={() => handleToggleStatus(tenant.id, tenant.status)} 
                                        title={tenant.status === 'active' ? "Suspend Tenant" : "Activate Tenant"}
                                    >
                                        <Power className="h-4 w-4" />
                                     </Button>
                                     <Button className="flex-1 group-hover:bg-primary group-hover:text-white transition-colors" onClick={() => handleManage(tenant.id)}>
                                        Manage Tenant <ArrowRight className="ml-2 h-4 w-4 group-hover:translate-x-1 transition-transform" />
                                     </Button>
                                </div>
                            </CardContent>
                        </Card>
                    ))}
                </div>
            )}

            <CreateTenantDialog 
                isOpen={isCreateOpen} 
                onClose={() => setIsCreateOpen(false)} 
                onSuccess={fetchTenants} 
            />
            
            <ConfirmDialog 
                isOpen={!!deleteId}
                title={`Delete Tenant ${deleteId}?`}
                description="This will permanently delete the database, all files, search indexes, and user data associated with this tenant. This action CANNOT be undone."
                confirmText="Delete Tenant"
                variant="destructive"
                onConfirm={handleDelete}
                onCancel={() => setDeleteId(null)}
                isLoading={isDeleting}
            />
        </div>
    );
};