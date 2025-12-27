import React, { useState, useEffect } from 'react';
import { Plus, Search, Building, ArrowRight, ExternalLink, Server, Database } from 'lucide-react';
import { Button, Input, Card, CardHeader, CardTitle, CardContent, Badge } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { Label } from '../../../components/form/FormPrimitives';
import { apiClient } from '../../../lib/apiClient';
import { useToast } from '../../../components/feedback/Toast';

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
    const [tenants, setTenants] = useState<string[]>([]);
    const [loading, setLoading] = useState(true);
    const [search, setSearch] = useState('');
    const [isCreateOpen, setIsCreateOpen] = useState(false);
    const { toast } = useToast();

    const fetchTenants = async () => {
        setLoading(true);
        try {
            const list = await apiClient.root.listTenants();
            setTenants(list);
        } catch (e) {
            console.error(e);
            toast('Failed to load tenants', 'error');
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        fetchTenants();
    }, []);

    const filtered = tenants.filter(t => t.toLowerCase().includes(search.toLowerCase()));

    // Deterministic color generator for avatars
    const getColor = (str: string) => {
        const colors = ['bg-blue-500', 'bg-purple-500', 'bg-emerald-500', 'bg-orange-500', 'bg-pink-500', 'bg-cyan-500'];
        let hash = 0;
        for (let i = 0; i < str.length; i++) hash = str.charCodeAt(i) + ((hash << 5) - hash);
        return colors[Math.abs(hash) % colors.length];
    };

    const handleManage = (id: string) => {
        // Navigate to Tenant Context
        window.location.href = `/_dashboard/tenant/${id}/dashboard`;
    };

    const handlePublicUrl = (id: string) => {
        // Assuming subdomain routing is set up
        const protocol = window.location.protocol;
        const host = window.location.host; // e.g. app.com or localhost:5000
        // For localhost/IP based dev, we can't easily jump to subdomain without DNS/Hosts config.
        // We'll fallback to showing the API endpoint or just a visual link.
        
        // Check if we are on localhost
        if (host.includes('localhost') || host.match(/\d{1,3}\.\d{1,3}\./)) {
             // Path based routing link
             window.open(`${protocol}//${host}/tenant/${id}/api/v1/collections`, '_blank');
        } else {
             // Subdomain routing link
             // Assume current host is "admin.app.com" or "app.com"
             // This logic depends on your specific domain setup
             window.open(`${protocol}//${id}.${host}`, '_blank');
        }
    };

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
                    {filtered.map(id => (
                        <Card key={id} className="group relative overflow-hidden transition-all duration-300 hover:shadow-xl hover:border-primary/50 hover:-translate-y-1">
                            {/* Decorative Background Gradient */}
                            <div className={`absolute top-0 inset-x-0 h-1 ${getColor(id)} opacity-80`} />
                            
                            <CardHeader className="pb-2">
                                <div className="flex justify-between items-start">
                                    <div className="flex items-center gap-3">
                                        <div className={`h-10 w-10 rounded-lg ${getColor(id)} bg-opacity-10 flex items-center justify-center text-white shadow-inner font-bold text-lg uppercase`}>
                                            {id.substring(0, 2)}
                                        </div>
                                        <div>
                                            <CardTitle className="text-lg">{id}</CardTitle>
                                            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mt-1">
                                                <div className="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
                                                Active
                                            </div>
                                        </div>
                                    </div>
                                    <Button variant="ghost" size="icon" className="text-muted-foreground hover:text-foreground" onClick={() => handlePublicUrl(id)} title="Public API">
                                        <ExternalLink className="h-4 w-4" />
                                    </Button>
                                </div>
                            </CardHeader>
                            
                            <CardContent>
                                <div className="grid grid-cols-2 gap-2 mt-2 mb-4">
                                    <div className="bg-secondary/30 p-2 rounded border border-border/50 flex flex-col gap-1">
                                        <span className="text-[10px] text-muted-foreground uppercase font-bold flex items-center gap-1">
                                            <Database className="h-3 w-3" /> Storage
                                        </span>
                                        <span className="text-xs font-mono">Isolated</span>
                                    </div>
                                    <div className="bg-secondary/30 p-2 rounded border border-border/50 flex flex-col gap-1">
                                        <span className="text-[10px] text-muted-foreground uppercase font-bold flex items-center gap-1">
                                            <Server className="h-3 w-3" /> Mode
                                        </span>
                                        <span className="text-xs font-mono">Dedicated</span>
                                    </div>
                                </div>
                                
                                <Button className="w-full group-hover:bg-primary group-hover:text-white transition-colors" onClick={() => handleManage(id)}>
                                    Manage Tenant <ArrowRight className="ml-2 h-4 w-4 group-hover:translate-x-1 transition-transform" />
                                </Button>
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
        </div>
    );
};