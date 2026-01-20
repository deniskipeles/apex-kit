import React, { useState, useEffect } from 'react';
import { Shield, Globe, Lock, Save, Plus, X } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Label, Switch, Button, Textarea, Input, Badge } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { Users } from 'lucide-react';
import { configService } from '@/src/features/settings/services/configService';
import { useToast } from '@/src/components/feedback/Toast';

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