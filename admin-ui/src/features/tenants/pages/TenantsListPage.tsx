import React, { useState, useEffect } from 'react';
import { Plus, Database, ExternalLink, Server } from 'lucide-react';
import { Button, Input, Card, CardHeader, CardTitle, CardContent, Badge } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { apiClient } from '../../../lib/apiClient';
import { useToast } from '../../../components/feedback/Toast';

export const TenantsListPage = () => {
  const [tenantId, setTenantId] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const { toast } = useToast();

  const [tenantsList, setTenantsList] = useState<string[]>([]); // Add state

  useEffect(() => {
    // Fetch on load
    apiClient.root.listTenants().then(setTenantsList);
  }, []);

  const handleCreate = async () => {
    if(!tenantId) return;
    setIsLoading(true);
    try {
        await apiClient.root.createTenant(tenantId);
        toast(`Tenant "${tenantId}" created successfully`, 'success');
        setTenantId('');
        setIsCreating(false);
    } catch (e: any) {
        toast(e.message || 'Failed to create tenant', 'error');
    } finally {
        setIsLoading(false);
    }
  };

  const navigateToTenant = (id: string) => {
      // Force a full page reload or handled via router to reset context if needed
      // But our apiClient proxy handles it based on URL path
      window.location.href = `/_dashboard/tenant/${id}`;
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Tenants</h2>
          <p className="text-muted-foreground">Manage isolated database instances.</p>
        </div>
        <Button onClick={() => setIsCreating(true)}>
          <Plus className="mr-2 h-4 w-4" /> Create Tenant
        </Button>
      </div>

      <Card className="border-dashed bg-secondary/10">
          <CardContent className="flex flex-col items-center justify-center py-12 text-center">
              <Server className="h-12 w-12 text-muted-foreground mb-4 opacity-50" />
              <h3 className="text-lg font-medium">Tenant Management</h3>
              <p className="text-sm text-muted-foreground max-w-sm mt-2">
                  Tenants are completely isolated environments with their own database, files, and search index. 
                  Enter a Tenant ID below to access or create one.
              </p>
              
              <div className="mt-8 flex gap-2 w-full max-w-md">
                 <Input 
                    placeholder="Enter Tenant ID to access (e.g. client-a)..." 
                    value={tenantId}
                    onChange={(e: any) => setTenantId(e.target.value)}
                 />
                 <Button onClick={() => navigateToTenant(tenantId)} disabled={!tenantId}>
                    Go <ExternalLink className="ml-2 h-4 w-4" />
                 </Button>
              </div>
          </CardContent>
      </Card>

      {/* Add List Section */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {tenantsList.map(id => (
            <Card key={id} className="cursor-pointer hover:border-primary/50 transition-all" onClick={() => navigateToTenant(id)}>
                <CardContent className="p-4 flex items-center justify-between">
                    <div className="flex items-center gap-3">
                        <div className="h-8 w-8 rounded bg-blue-500/10 text-blue-500 flex items-center justify-center font-bold text-xs">
                            {id.substring(0, 2).toUpperCase()}
                        </div>
                        <span className="font-medium">{id}</span>
                    </div>
                    <ExternalLink className="h-4 w-4 text-muted-foreground" />
                </CardContent>
            </Card>
        ))}
      </div>

      <Dialog isOpen={isCreating} onClose={() => setIsCreating(false)} title="Provision New Tenant">
          <div className="space-y-4">
              <div className="space-y-2">
                  <label className="text-sm font-medium">Tenant ID (Subdomain/Slug)</label>
                  <Input 
                      placeholder="e.g. organization-1" 
                      value={tenantId} 
                      onChange={(e: any) => setTenantId(e.target.value)} 
                      autoFocus
                  />
                  <p className="text-xs text-muted-foreground">
                      This ID will be used for API routing (e.g. /tenant/<b>{tenantId || '...'}</b>/api/v1).
                      It must be URL safe.
                  </p>
              </div>
              <div className="flex justify-end gap-2 pt-2">
                  <Button variant="ghost" onClick={() => setIsCreating(false)}>Cancel</Button>
                  <Button onClick={handleCreate} isLoading={isLoading} disabled={!tenantId}>Provision Database</Button>
              </div>
          </div>
      </Dialog>
    </div>
  );
};