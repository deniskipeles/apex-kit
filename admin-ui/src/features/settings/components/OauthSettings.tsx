import React, { useState, useEffect } from 'react';
import { Save, Github, Globe, RefreshCw, Key, Copy, Check, ExternalLink } from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Input,
  Label,
  Button,
} from '../../../components/ui/Elements';
import { PasswordInput } from '../../../components/form/PasswordInput';
import { configService } from '../services/configService';
import { useToast } from '../../../components/feedback/Toast';

export const OauthSettings = () => {
  const { toast } = useToast();

  // Form State
  const [githubId, setGithubId] = useState('');
  const [githubSecret, setGithubSecret] = useState('');
  const [googleId, setGoogleId] = useState('');
  const [googleSecret, setGoogleSecret] = useState('');

  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Dynamic Scope Detector
  const getActiveScope = () => {
    const path = window.location.pathname;
    const tenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)/);
    if (tenantMatch) {
      return { type: 'tenant', id: tenantMatch[1] };
    }
    const sandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)/);
    if (sandboxMatch) {
      return { type: 'sandbox', id: sandboxMatch[1] };
    }
    return { type: 'root', id: 'root' };
  };

  const scope = getActiveScope();
  const origin = window.location.origin; // e.g. https://kipeles-vs--5000.hf.space
  const host = window.location.host; // e.g. kipeles-vs--5000.hf.space
  const protocol = window.location.protocol; // e.g. https:

  // Generate exact callback targets
  const getCallbackUrls = (provider: 'github' | 'google') => {
    const suffix = `api/v1/auth/${provider}/callback`;

    if (scope.type === 'root') {
      return {
        path: `${origin}/${suffix}`,
        subdomain: null,
      };
    }

    const scopePath =
      scope.type === 'sandbox' ? `sandbox/session_${scope.id}` : `tenant/${scope.id}`;

    return {
      path: `${origin}/${scopePath}/${suffix}`,
      subdomain: `${protocol}//${scope.id}.${host}/${suffix}`,
    };
  };

  const ghUrls = getCallbackUrls('github');
  const ggUrls = getCallbackUrls('google');

  const loadConfigs = async () => {
    setIsLoading(true);
    try {
      const list = await configService.list();

      const ghId = list.find((c) => c.key === 'github_client_id');
      const ghSec = list.find((c) => c.key === 'github_client_secret');
      const ggId = list.find((c) => c.key === 'google_client_id');
      const ggSec = list.find((c) => c.key === 'google_client_secret');

      setGithubId(ghId?.value || '');
      setGithubSecret(ghSec?.encrypted ? '******' : ghSec?.value || '');
      setGoogleId(ggId?.value || '');
      setGoogleSecret(ggSec?.encrypted ? '******' : ggSec?.value || '');
    } catch (e) {
      toast('Failed to load OAuth2 configurations', 'error');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadConfigs();
  }, []);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      if (githubId) {
        await configService.set('github_client_id', githubId, false);
      }
      if (githubSecret && githubSecret !== '******') {
        await configService.set('github_client_secret', githubSecret, true);
      }

      if (googleId) {
        await configService.set('google_client_id', googleId, false);
      }
      if (googleSecret && googleSecret !== '******') {
        await configService.set('google_client_secret', googleSecret, true);
      }

      toast('OAuth2 configurations securely updated', 'success');
      loadConfigs();
    } catch (e) {
      toast('Failed to save credentials', 'error');
    } finally {
      setIsSaving(false);
    }
  };

  const handleCopy = (text: string, key: string) => {
    navigator.clipboard.writeText(text);
    setCopiedKey(key);
    toast('Copied to clipboard', 'success');
    setTimeout(() => setCopiedKey(null), 2000);
  };

  const renderUrlHelper = (urls: { path: string; subdomain: string | null }, idPrefix: string) => {
    return (
      <div className="mt-4 p-3 bg-secondary/15 rounded-lg border border-border space-y-2.5">
        <div className="text-[10px] font-bold text-muted-foreground uppercase tracking-wider">
          Required Redirect configurations
        </div>

        <div className="space-y-2">
          {/* Path-based URL */}
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 text-xs font-mono">
            <span className="text-muted-foreground shrink-0 w-24">Path Based:</span>
            <div className="flex items-center gap-1.5 bg-background p-1 px-2 rounded border border-border/50 w-full min-w-0">
              <span className="truncate text-foreground/80 flex-1">{urls.path}</span>
              <button
                type="button"
                onClick={() => handleCopy(urls.path, `${idPrefix}_path`)}
                className="text-muted-foreground hover:text-foreground shrink-0 p-1 hover:bg-secondary rounded transition"
              >
                {copiedKey === `${idPrefix}_path` ? (
                  <Check className="h-3.5 w-3.5 text-green-500" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
          </div>

          {/* Subdomain-based URL */}
          {urls.subdomain && (
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 text-xs font-mono">
              <span className="text-muted-foreground shrink-0 w-24">Subdomain:</span>
              <div className="flex items-center gap-1.5 bg-background p-1 px-2 rounded border border-border/50 w-full min-w-0">
                <span className="truncate text-foreground/80 flex-1">{urls.subdomain}</span>
                <button
                  type="button"
                  onClick={() => handleCopy(urls.subdomain!, `${idPrefix}_sub`)}
                  className="text-muted-foreground hover:text-foreground shrink-0 p-1 hover:bg-secondary rounded transition"
                >
                  {copiedKey === `${idPrefix}_sub` ? (
                    <Check className="h-3.5 w-3.5 text-green-500" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* GitHub Section */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Github className="h-5 w-5 text-neutral-800 dark:text-neutral-100" /> GitHub OAuth2
            Integration
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="p-3 bg-secondary/10 rounded-lg border border-border text-xs flex flex-col sm:flex-row justify-between items-start sm:items-center gap-2">
            <span className="text-muted-foreground font-semibold">
              To configure, register an OAuth application under your GitHub account:
            </span>
            <a
              href="https://github.com/settings/developers"
              target="_blank"
              rel="noreferrer"
              className="text-primary hover:underline flex items-center gap-1 font-bold shrink-0"
            >
              GitHub Developer Portal <ExternalLink className="h-3.5 w-3.5" />
            </a>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label>Client ID</Label>
              <Input
                value={githubId}
                onChange={(e: any) => setGithubId(e.target.value)}
                placeholder="e.g. Iv1.33b8a6a12b..."
                disabled={isLoading || isSaving}
                className="font-mono text-sm"
              />
            </div>
            <div className="space-y-2">
              <Label>Client Secret</Label>
              <PasswordInput
                value={githubSecret}
                onChange={(e: any) => setGithubSecret(e.target.value)}
                placeholder={githubSecret === '******' ? '••••••••' : 'Enter client secret...'}
                disabled={isLoading || isSaving}
              />
            </div>
          </div>
          {renderUrlHelper(ghUrls, 'github')}
        </CardContent>
      </Card>

      {/* Google Section */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Globe className="h-5 w-5 text-blue-500" /> Google OAuth2 Integration
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="p-3 bg-secondary/10 rounded-lg border border-border text-xs flex flex-col sm:flex-row justify-between items-start sm:items-center gap-2">
            <span className="text-muted-foreground font-semibold">
              To configure, generate an OAuth 2.0 client credential inside your Google Cloud
              project:
            </span>
            <a
              href="https://console.cloud.google.com/apis/credentials"
              target="_blank"
              rel="noreferrer"
              className="text-primary hover:underline flex items-center gap-1 font-bold shrink-0"
            >
              Google Cloud Credentials <ExternalLink className="h-3.5 w-3.5" />
            </a>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label>Client ID</Label>
              <Input
                value={googleId}
                onChange={(e: any) => setGoogleId(e.target.value)}
                placeholder="e.g. 1083321-abc123xyz.apps.googleusercontent.com"
                disabled={isLoading || isSaving}
                className="font-mono text-sm"
              />
            </div>
            <div className="space-y-2">
              <Label>Client Secret</Label>
              <PasswordInput
                value={googleSecret}
                onChange={(e: any) => setGoogleSecret(e.target.value)}
                placeholder={googleSecret === '******' ? '••••••••' : 'Enter client secret...'}
                disabled={isLoading || isSaving}
              />
            </div>
          </div>
          {renderUrlHelper(ggUrls, 'google')}
        </CardContent>
      </Card>

      {/* Security Info Card */}
      <div className="p-4 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-xs text-emerald-500 flex items-start gap-2.5 shadow-sm">
        <Key className="h-4.5 w-4.5 mt-0.5 shrink-0" />
        <div className="space-y-1">
          <p className="font-bold uppercase tracking-wider">Secure Hardware Vault Active</p>
          <p className="opacity-90 leading-relaxed font-semibold">
            All OAuth Client Secrets are dynamically encrypted inside your SQLite database using
            your master key. They are safely decrypted inside secure system memory only during the
            authorization handshake.
          </p>
        </div>
      </div>

      <div className="flex justify-end pt-2">
        <Button
          onClick={handleSave}
          isLoading={isSaving}
          disabled={isLoading}
          className="w-full sm:w-auto"
        >
          <Save className="mr-2 h-4 w-4" /> Save OAuth2 Settings
        </Button>
      </div>
    </div>
  );
};
