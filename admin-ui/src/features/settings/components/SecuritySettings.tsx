import React, { useState, useEffect } from 'react';
import {
  Shield,
  Globe,
  Lock,
  Save,
  Plus,
  X,
  Database,
  Activity,
  FileJson,
  Type,
} from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Label,
  Switch,
  Button,
  Textarea,
  Input,
  Badge,
} from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { Users } from 'lucide-react';
import { configService } from '@/src/features/settings/services/configService';
import { useToast } from '@/src/components/feedback/Toast';
import { apiClient } from '@/src/lib/apiClient';
import { JSONEditor } from '../../../components/form/JsonEditor'; // <--- IMPORT JSON EDITOR

interface SecuritySettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

// Helper to check if a string is valid JSON
const isJsonString = (str: string) => {
  try {
    const parsed = JSON.parse(str);
    return typeof parsed === 'object' && parsed !== null;
  } catch (e) {
    return false;
  }
};

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
    delete: 'admin',
  });

  // [NEW] Mode states for each policy field (true = JSON, false = Legacy String)
  const [policyModes, setPolicyModes] = useState({
    read: false,
    create: false,
    update: false,
    delete: false,
  });

  const [isLoadingPolicies, setIsLoadingPolicies] = useState(true);

  // Load Policies on mount
  useEffect(() => {
    const loadPolicies = async () => {
      try {
        const configs = await configService.list();
        const policyConf = configs.find((c) => c.key === 'policy_users');
        if (policyConf && policyConf.value) {
          const parsed = JSON.parse(policyConf.value);
          setUserPolicies(parsed);

          // Auto-detect if saved policies are JSON objects or legacy strings
          setPolicyModes({
            read: isJsonString(parsed.read),
            create: isJsonString(parsed.create),
            update: isJsonString(parsed.update),
            delete: isJsonString(parsed.delete),
          });
        }
      } catch (e) {
        console.error(e);
      } finally {
        setIsLoadingPolicies(false);
      }
    };
    loadPolicies();
  }, []);

  // ... (Load Roles and Role functions remain exactly the same) ...
  useEffect(() => {
    const loadRoles = async () => {
      try {
        const list = await configService.list();
        const roleConfig = list.find((c) => c.key === '_apexkit_auth_roles');
        if (roleConfig && roleConfig.value) {
          setRoles(JSON.parse(roleConfig.value));
        } else {
          setRoles(['admin', 'user']); // Default
        }
      } catch (e) {}
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
      toast('Cannot delete default roles', 'error');
      return;
    }
    const updated = roles.filter((r) => r !== role);
    setRoles(updated);
    saveRoles(updated);
  };

  const saveRoles = async (updatedRoles: string[]) => {
    try {
      await configService.set('_apexkit_auth_roles', JSON.stringify(updatedRoles), false);
      toast('Roles updated', 'success');
    } catch (e) {
      toast('Failed to save roles', 'error');
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
        security: settings.security,
      });
    } finally {
      setIsSaving(false);
    }
  };

  // Save Handler for Policies
  const savePolicies = async () => {
    try {
      // Validate JSON fields before saving
      for (const key of ['read', 'create', 'update', 'delete'] as const) {
        if (policyModes[key]) {
          try {
            const parsed = JSON.parse(userPolicies[key]);
            // Minify for clean DB storage
            userPolicies[key] = JSON.stringify(parsed);
          } catch (e) {
            toast(`Invalid JSON in ${key} policy`, 'error');
            return;
          }
        }
      }

      await configService.set('policy_users', JSON.stringify(userPolicies), false);
      toast('User policies updated', 'success');
    } catch (e) {
      toast('Failed to save policies', 'error');
    }
  };

  // [NEW] Helper to render the unified policy input
  const renderPolicyInput = (
    key: 'read' | 'create' | 'update' | 'delete',
    label: string,
    placeholder: string
  ) => {
    const isJson = policyModes[key];
    const value = userPolicies[key];

    const toggleMode = (toJson: boolean) => {
      if (toJson && !isJson) {
        // Attempt to stringify if it's somehow already an object, or provide default JSON
        setUserPolicies({ ...userPolicies, [key]: '{\n  \n}' });
      } else if (!toJson && isJson) {
        setUserPolicies({ ...userPolicies, [key]: '' });
      }
      setPolicyModes({ ...policyModes, [key]: toJson });
    };

    return (
      <div className="space-y-1.5 flex flex-col h-full">
        <div className="flex justify-between items-center mb-1">
          <Label className="capitalize">{label}</Label>
          <div className="flex items-center gap-1 bg-secondary/30 p-0.5 rounded-lg border border-border">
            <button
              onClick={() => toggleMode(false)}
              className={`px-2 py-0.5 rounded text-[10px] flex items-center gap-1 transition-all ${!isJson ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <Type className="h-3 w-3" /> Legacy
            </button>
            <button
              onClick={() => toggleMode(true)}
              className={`px-2 py-0.5 rounded text-[10px] flex items-center gap-1 transition-all ${isJson ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <FileJson className="h-3 w-3" /> JSON (Advanced)
            </button>
          </div>
        </div>

        {isJson ? (
          <div className="border border-input rounded-md overflow-hidden flex-1 min-h-[120px]">
            <JSONEditor
              value={value}
              onChange={(val) => setUserPolicies({ ...userPolicies, [key]: val })}
              height="100%"
            />
          </div>
        ) : (
          <Input
            value={value}
            onChange={(e: any) => setUserPolicies({ ...userPolicies, [key]: e.target.value })}
            className="font-mono text-xs"
            placeholder={placeholder}
          />
        )}
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* ... Roles Management Card ... */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Users className="h-4 w-4" /> User Roles
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex flex-wrap gap-2">
              {roles.map((role) => (
                <Badge
                  key={role}
                  variant="secondary"
                  className="px-3 py-1 text-sm flex items-center gap-2"
                >
                  {role}
                  {role !== 'admin' && role !== 'user' && (
                    <button onClick={() => removeRole(role)} className="hover:text-destructive">
                      <X className="h-3 w-3" />
                    </button>
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
              <Button onClick={addRole} disabled={!newRole}>
                <Plus className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* ... Access Control Card ... */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Shield className="h-4 w-4" /> Access Control
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col sm:flex-row sm:items-center justify-between rounded-lg border border-border p-4 gap-4">
            <div className="space-y-0.5">
              <Label className="text-base">Allow Public Registration</Label>
              <p className="text-sm text-muted-foreground">
                If enabled, anyone can create an account.
              </p>
            </div>
            <Switch
              checked={settings.allowPublicRegistration}
              onCheckedChange={(c: boolean) => onChange({ allowPublicRegistration: c })}
            />
          </div>
          {apiClient.getScope().type === 'root' && (
            <div className="flex flex-col sm:flex-row sm:items-center justify-between rounded-lg border border-border p-4 gap-4 mt-4">
              <div className="space-y-0.5">
                <Label className="text-base flex items-center gap-2">
                  Tenant Transparency Mode
                </Label>
                <p className="text-sm text-muted-foreground">
                  If enabled, tenants can see the names of all root-level system scripts (code is
                  securely redacted). Helps build trust in your platform.
                </p>
              </div>
              <Switch
                checked={settings.security.tenantTransparency}
                onCheckedChange={(c: boolean) => updateSecurity('tenantTransparency', c)}
              />
            </div>
          )}
        </CardContent>
      </Card>

      {/* [UPDATED] User Data Policies Card */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-4 w-4" /> User Data Policies
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="text-sm text-muted-foreground bg-secondary/10 p-3 rounded-md border border-border leading-relaxed">
              Define who can access the system `users` table.
              <br />- <b>Legacy Mode:</b> Use strings like <code>admin</code>, <code>auth</code>,{' '}
              <code>public</code>, or <code>owner:id</code>.
              <br />- <b>JSON Mode:</b> Build advanced relational queries using{' '}
              <code>{`{ "$in": { "@get()": { ... } } }`}</code> syntax.
            </div>

            {isLoadingPolicies ? (
              <div className="p-4 text-center text-xs text-muted-foreground">
                Loading policies...
              </div>
            ) : (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                {renderPolicyInput('read', 'Read (List/Get)', 'admin')}
                {renderPolicyInput('create', 'Create (Register)', 'public')}
                {renderPolicyInput('update', 'Update (Edit)', 'admin || owner:id')}
                {renderPolicyInput('delete', 'Delete', 'admin')}
              </div>
            )}
            <div className="flex justify-end pt-2 border-t border-border mt-2">
              <Button size="sm" variant="outline" onClick={savePolicies}>
                Update Policies
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Rate Limiting Card (Scope Aware) */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Activity className="h-4 w-4" /> Rate Limiting
            <Badge variant="outline" className="text-[10px] font-mono uppercase ml-1">
              {apiClient.getScope().type}
            </Badge>
          </CardTitle>
        </CardHeader>
        <CardContent>
          {/* ROOT SCOPE: Show Global API limit + Tier presets */}
          {apiClient.getScope().type === 'root' && (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              <div className="space-y-2">
                <Label>Global / Root API Limit</Label>
                <Input
                  type="number"
                  placeholder="e.g. 600"
                  value={settings.security.globalRateLimit || 600}
                  onChange={(e: any) => updateSecurity('globalRateLimit', Number(e.target.value))}
                />
                <p className="text-[10px] text-muted-foreground">
                  Requests per minute per IP on Root API.
                </p>
              </div>
              <div className="space-y-2">
                <Label>Tenant (Free Tier) Limit</Label>
                <Input
                  type="number"
                  placeholder="e.g. 120"
                  value={settings.security.tenantFreeRateLimit || 120}
                  onChange={(e: any) =>
                    updateSecurity('tenantFreeRateLimit', Number(e.target.value))
                  }
                />
                <p className="text-[10px] text-muted-foreground">
                  Default for tenants on 'free' tier.
                </p>
              </div>
              <div className="space-y-2">
                <Label>Tenant (Pro Tier) Limit</Label>
                <Input
                  type="number"
                  placeholder="e.g. 3000"
                  value={settings.security.tenantProRateLimit || 3000}
                  onChange={(e: any) =>
                    updateSecurity('tenantProRateLimit', Number(e.target.value))
                  }
                />
                <p className="text-[10px] text-muted-foreground">
                  Default for tenants on 'pro' tier.
                </p>
              </div>
            </div>
          )}

          {/* TENANT SCOPE: Show only the rate limit form for this tenant */}
          {apiClient.getScope().type === 'tenant' && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div className="space-y-2">
                <Label>Tenant API Rate Limit</Label>
                <Input
                  type="number"
                  placeholder="e.g. 120"
                  value={settings.security.tenantFreeRateLimit || 120}
                  onChange={(e: any) =>
                    updateSecurity('tenantFreeRateLimit', Number(e.target.value))
                  }
                />
                <p className="text-[10px] text-muted-foreground">
                  Requests per minute per client IP for this tenant.
                </p>
              </div>
            </div>
          )}

          {/* SANDBOX SCOPE: Show sandbox-specific notice */}
          {apiClient.getScope().type === 'sandbox' && (
            <div className="p-3.5 bg-secondary/15 rounded-lg border border-border text-xs text-muted-foreground">
              <p className="font-semibold text-foreground text-sm">Sandbox Rate Limit</p>
              <p className="mt-1">
                Sandbox environments are locked to <strong>60 requests per minute</strong> per IP to
                protect ephemeral session resources.
              </p>
            </div>
          )}
        </CardContent>
      </Card>

      {/* ... CORS Settings Card ... */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <CardTitle className="flex items-center gap-2">
                <Globe className="h-4 w-4" /> CORS Configuration
              </CardTitle>
              <p className="text-xs text-muted-foreground">
                Control which websites can access your API.
              </p>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs font-medium text-muted-foreground">
                {settings.security.corsAllowAll ? 'Public API' : 'Restricted'}
              </span>
              <Switch
                checked={settings.security.corsAllowAll}
                onCheckedChange={(c: boolean) => updateSecurity('corsAllowAll', c)}
              />
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div
            className={`space-y-4 transition-all duration-300 ${settings.security.corsAllowAll ? 'opacity-50 pointer-events-none grayscale' : 'opacity-100'}`}
          >
            <div className="space-y-2">
              <Label>Allowed Origins</Label>
              <Textarea
                value={settings.security.corsOrigins}
                onChange={(e: any) => updateSecurity('corsOrigins', e.target.value)}
                placeholder="https://myapp.com, http://localhost:3000"
                className="font-mono text-xs min-h-[80px]"
              />
              <p className="text-[10px] text-muted-foreground">
                Comma separated list of full URLs.
              </p>
            </div>
          </div>
          {settings.security.corsAllowAll && (
            <div className="mt-4 p-3 bg-amber-500/10 border border-amber-500/20 rounded-md text-xs text-amber-600 flex items-center gap-2">
              <Lock className="h-3 w-3" /> Warning: Your API is currently accessible from any
              website.
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
