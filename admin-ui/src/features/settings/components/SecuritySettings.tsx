import React, { useState, useEffect } from 'react';
import { Shield, Globe, Lock, Save, Plus, X, Database } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Label, Switch, Button, Textarea, Input, Badge } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { Users } from 'lucide-react';
import { configService } from '@/src/features/settings/services/configService';
import { useToast } from '@/src/components/feedback/Toast';
import { apiClient } from '@/src/lib/apiClient';

interface SecuritySettingsProps {
    settings: AppSettings;
    onChange: (settings: Partial<AppSettings>) => void;
    onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const SecuritySettings = ({ settings, onChange, onSave }: SecuritySettingsProps) => {
    const [isSaving, setIsSaving] = useState(false);
    // State for roles
    const [roles, setRoles] = useState<string[]>([]);
    const [newRole, setNewRole] = useState('');
    const { toast } = useToast();
    //  User Policy State
    const [userPolicies, setUserPolicies] = useState({
        read: 'admin || owner:id',
        create: 'public',
        update: 'admin || owner:id',
        delete: 'admin'
    });
    const [isLoadingPolicies, setIsLoadingPolicies] = useState(true);

    // Load Policies on mount
    useEffect(() => {
        const loadPolicies = async () => {
            try {
                const configs = await configService.list();
                const policyConf = configs.find(c => c.key === 'policy_users');
                if (policyConf && policyConf.value) {
                    const parsed = JSON.parse(policyConf.value);
                    setUserPolicies(parsed);
                }
            } catch (e) { console.error(e); }
            finally { setIsLoadingPolicies(false); }
        };
        loadPolicies();
    }, []);

    // Load roles on mount
    useEffect(() => {
        const loadRoles = async () => {
            try {
                // We can use the listRoles endpoint we created
                // Or fetch the config directly. Let's use config service for editing.
                const list = await configService.list();
                const roleConfig = list.find(c => c.key === 'APEX_AUTH_ROLES');
                if (roleConfig && roleConfig.value) {
                    setRoles(JSON.parse(roleConfig.value));
                } else {
                    setRoles(['admin', 'user']); // Default
                }
            } catch (e) { }
        };
        loadRoles();
    }, []);

    const addRole = () => {
        if (newRole && !roles.includes(newRole)) {
            const updated = [...roles, newRole];
            setRoles(updated);
            setNewRole('');
            saveRoles(updated);
        }
    };

    const removeRole = (role: string) => {
        if (role === 'admin' || role === 'user') {
            toast("Cannot delete default roles", "error");
            return;
        }
        const updated = roles.filter(r => r !== role);
        setRoles(updated);
        saveRoles(updated);
    };

    const saveRoles = async (updatedRoles: string[]) => {
        try {
            await configService.set('APEX_AUTH_ROLES', JSON.stringify(updatedRoles), false);
            toast("Roles updated", "success");
        } catch (e) {
            toast("Failed to save roles", "error");
        }
    };

    const updateSecurity = (key: string, value: any) => {
        onChange({ security: { ...settings.security, [key]: value } });
    };

    const handleSaveClick = async () => {
        setIsSaving(true);
        try {
            await onSave({
                allowPublicRegistration: settings.allowPublicRegistration,
                security: settings.security
            });
        } finally {
            setIsSaving(false);
        }
    };

    // Save Handler for Policies (Separate from main settings save for clarity/modularity)
    const savePolicies = async () => {
        try {
            await configService.set('policy_users', JSON.stringify(userPolicies), false);
            toast('User policies updated', 'success');
        } catch (e) {
            toast('Failed to save policies', 'error');
        }
    };

    return (
        <div className="space-y-6">
            {/* Roles Management Card */}
            <Card>
                <CardHeader>
                    <CardTitle className="flex items-center gap-2"><Users className="h-4 w-4" /> User Roles</CardTitle>
                </CardHeader>
                <CardContent>
                    <div className="space-y-4">
                        <div className="flex flex-wrap gap-2">
                            {roles.map(role => (
                                <Badge key={role} variant="secondary" className="px-3 py-1 text-sm flex items-center gap-2">
                                    {role}
                                    {role !== 'admin' && role !== 'user' && (
                                        <button onClick={() => removeRole(role)} className="hover:text-destructive"><X className="h-3 w-3" /></button>
                                    )}
                                </Badge>
                            ))}
                        </div>
                        <div className="flex gap-2 max-w-sm">
                            <Input
                                placeholder="New Role (e.g. editor)"
                                value={newRole}
                                onChange={(e: any) => setNewRole(e.target.value.toLowerCase())}
                                onKeyDown={(e: any) => e.key === 'Enter' && addRole()}
                            />
                            <Button onClick={addRole} disabled={!newRole}><Plus className="h-4 w-4" /></Button>
                        </div>
                    </div>
                </CardContent>
            </Card>

            {/* Access Control Card */}
            <Card>
                <CardHeader>
                    <CardTitle className="flex items-center gap-2"><Shield className="h-4 w-4" /> Access Control</CardTitle>
                </CardHeader>
                <CardContent>
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between rounded-lg border border-border p-4 gap-4">
                        <div className="space-y-0.5">
                            <Label className="text-base">Allow Public Registration</Label>
                            <p className="text-sm text-muted-foreground">If enabled, anyone can create an account.</p>
                        </div>
                        <Switch
                            checked={settings.allowPublicRegistration}
                            onCheckedChange={(c: boolean) => onChange({ allowPublicRegistration: c })}
                        />
                    </div>
                    {apiClient.getScope().type === 'root' && (
                        <div className="flex flex-col sm:flex-row sm:items-center justify-between rounded-lg border border-border p-4 gap-4 mt-4">
                            <div className="space-y-0.5">
                                <Label className="text-base flex items-center gap-2">Tenant Transparency Mode</Label>
                                <p className="text-sm text-muted-foreground">If enabled, tenants can see the names of all root-level system scripts (code is securely redacted). Helps build trust in your platform.</p>
                            </div>
                            <Switch
                                checked={settings.security.tenantTransparency}
                                onCheckedChange={(c: boolean) => updateSecurity('tenantTransparency', c)}
                            />
                        </div>)}
                </CardContent>
            </Card>

            {/* [NEW] User Data Policies Card */}
            <Card>
                <CardHeader>
                    <CardTitle className="flex items-center gap-2"><Database className="h-4 w-4" /> User Data Policies</CardTitle>
                </CardHeader>
                <CardContent>
                    <div className="space-y-4">
                        <div className="text-sm text-muted-foreground bg-secondary/10 p-3 rounded-md border border-border">
                            Define who can access the system `users` table.
                            Use rules like <code>admin</code>, <code>auth</code>, <code>public</code>, or <code>owner:id</code>.
                        </div>

                        {isLoadingPolicies ? (
                            <div className="p-4 text-center text-xs text-muted-foreground">Loading policies...</div>
                        ) : (
                            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                                <div className="space-y-1">
                                    <Label>Read (List/Get)</Label>
                                    <Input
                                        value={userPolicies.read}
                                        onChange={(e: any) => setUserPolicies({ ...userPolicies, read: e.target.value })}
                                        className="font-mono text-xs"
                                        placeholder="admin"
                                    />
                                </div>
                                <div className="space-y-1">
                                    <Label>Create (Register)</Label>
                                    <Input
                                        value={userPolicies.create}
                                        onChange={(e: any) => setUserPolicies({ ...userPolicies, create: e.target.value })}
                                        className="font-mono text-xs"
                                        placeholder="public"
                                    />
                                </div>
                                <div className="space-y-1">
                                    <Label>Update (Edit)</Label>
                                    <Input
                                        value={userPolicies.update}
                                        onChange={(e: any) => setUserPolicies({ ...userPolicies, update: e.target.value })}
                                        className="font-mono text-xs"
                                        placeholder="admin || owner:id"
                                    />
                                </div>
                                <div className="space-y-1">
                                    <Label>Delete</Label>
                                    <Input
                                        value={userPolicies.delete}
                                        onChange={(e: any) => setUserPolicies({ ...userPolicies, delete: e.target.value })}
                                        className="font-mono text-xs"
                                        placeholder="admin"
                                    />
                                </div>
                            </div>
                        )}
                        <div className="flex justify-end pt-2">
                            <Button size="sm" variant="outline" onClick={savePolicies}>Update Policies</Button>
                        </div>
                    </div>
                </CardContent>
            </Card>

            {/* CORS Settings Card */}
            <Card>
                <CardHeader>
                    <div className="flex items-center justify-between">
                        <div className="space-y-1">
                            <CardTitle className="flex items-center gap-2"><Globe className="h-4 w-4" /> CORS Configuration</CardTitle>
                            <p className="text-xs text-muted-foreground">Control which websites can access your API.</p>
                        </div>
                        <div className="flex items-center gap-2">
                            <span className="text-xs font-medium text-muted-foreground">{settings.security.corsAllowAll ? 'Public API' : 'Restricted'}</span>
                            <Switch
                                checked={settings.security.corsAllowAll}
                                onCheckedChange={(c: boolean) => updateSecurity('corsAllowAll', c)}
                            />
                        </div>
                    </div>
                </CardHeader>
                <CardContent>
                    <div className={`space-y-4 transition-all duration-300 ${settings.security.corsAllowAll ? 'opacity-50 pointer-events-none grayscale' : 'opacity-100'}`}>
                        <div className="space-y-2">
                            <Label>Allowed Origins</Label>
                            <Textarea
                                value={settings.security.corsOrigins}
                                onChange={(e: any) => updateSecurity('corsOrigins', e.target.value)}
                                placeholder="https://myapp.com, http://localhost:3000"
                                className="font-mono text-xs min-h-[80px]"
                            />
                            <p className="text-[10px] text-muted-foreground">Comma separated list of full URLs.</p>
                        </div>
                    </div>
                    {settings.security.corsAllowAll && (
                        <div className="mt-4 p-3 bg-amber-500/10 border border-amber-500/20 rounded-md text-xs text-amber-600 flex items-center gap-2">
                            <Lock className="h-3 w-3" /> Warning: Your API is currently accessible from any website.
                        </div>
                    )}
                </CardContent>
            </Card>

            {/* Global Save for this Tab */}
            <div className="flex justify-end">
                <Button onClick={handleSaveClick} isLoading={isSaving} className="w-full sm:w-auto">
                    <Save className="mr-2 h-4 w-4" /> Save Security Settings
                </Button>
            </div>
        </div>
    );
};