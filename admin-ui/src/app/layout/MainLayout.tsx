import React, { useState, useEffect, useMemo } from 'react';
import { Loader2 } from 'lucide-react';
import { useAuth } from '../../hooks/useAuth';
import { LoginPage } from '../../features/auth/pages/LoginPage';
import { Sidebar } from '../../features/navigation/Sidebar';
import { Topbar } from '../../features/navigation/Topbar';
import { Breadcrumb } from '../../components/navigation/Breadcrumb';
import { Router } from '../routes';
import { SandboxAiToolbar } from '../../features/ai/components/SandboxAiToolbar';
import { getSubdomainTenant } from '@/src/lib/subdomain';

export const MainLayout = () => {
  const { user, isLoading, checkAuth } = useAuth();

  // 1. Initialize View State from Subdomain, Injected Scope, or URL
  const getInitialView = () => {
    const subTenant = getSubdomainTenant();
    const backendScope = typeof window !== 'undefined' ? window.__APEX_SCOPE__ : null;
    const path = window.location.pathname;

    // A. Subdomain mode (e.g. apexkit-drive.kipeles.dev/_dashboard/collections)
    if (subTenant) {
      const match = path.match(/^\/_dashboard(?:\/(.*))?/);
      const subPath = match && match[1] ? match[1] : 'dashboard';
      return `tenant__${subTenant}__${subPath}`;
    }

    // B. Direct Scoped Path (/tenant/:id/_dashboard or /sandbox/:id/_dashboard)
    if (backendScope && backendScope.type === 'tenant' && backendScope.id) {
      const match =
        path.match(/^\/tenant\/[^/]+\/_dashboard(?:\/(.*))?/) ||
        path.match(/^\/_dashboard(?:\/(.*))?/);
      const subPath = match && match[1] ? match[1] : 'dashboard';
      return `tenant__${backendScope.id}__${subPath}`;
    }

    // C. Root Admin drilldown (/ _dashboard/tenant/:id/... or /_dashboard/sandbox/:id/...)
    const rootTenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)\/?(.*)/);
    if (rootTenantMatch) {
      const subPath = rootTenantMatch[2] || 'dashboard';
      return `tenant__${rootTenantMatch[1]}__${subPath}`;
    }

    const rootSandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)\/?(.*)/);
    if (rootSandboxMatch) {
      const subPath = rootSandboxMatch[2] || 'dashboard';
      return `sandbox__${rootSandboxMatch[1]}__${subPath}`;
    }

    const dashboardMatch = path.match(/^\/_dashboard\/?(.*)/);
    return dashboardMatch && dashboardMatch[1] ? dashboardMatch[1] : 'dashboard';
  };

  const [currentView, setCurrentView] = useState(getInitialView());

  // 2. Sync URL with State Changes (Preserves existing URL structure without unexpected rewrites)
  useEffect(() => {
    const path = window.location.pathname;
    let newPath = '/_dashboard';

    const isDirectTenant = path.startsWith('/tenant/');
    const isDirectSandbox = path.startsWith('/sandbox/');

    if (currentView.startsWith('tenant__')) {
      const parts = currentView.split('__');
      if (parts.length >= 3) {
        const tenantId = parts[1];
        const sub = parts[2] === 'dashboard' ? '' : parts[2];
        if (isDirectTenant) {
          newPath = `/tenant/${tenantId}/_dashboard${sub ? `/${sub}` : ''}`;
        } else {
          newPath = `/_dashboard/tenant/${tenantId}/${parts[2]}`;
        }
      }
    } else if (currentView.startsWith('sandbox__')) {
      const parts = currentView.split('__');
      if (parts.length >= 3) {
        const sandboxId = parts[1];
        const sub = parts[2] === 'dashboard' ? '' : parts[2];
        if (isDirectSandbox) {
          newPath = `/sandbox/${sandboxId}/_dashboard${sub ? `/${sub}` : ''}`;
        } else {
          newPath = `/_dashboard/sandbox/${sandboxId}/${parts[2]}`;
        }
      }
    } else {
      newPath = `/_dashboard/${currentView === 'dashboard' ? '' : currentView}`;
    }

    if (newPath.endsWith('/') && newPath !== '/') {
      newPath = newPath.slice(0, -1);
    }

    if (path !== newPath) {
      window.history.pushState({}, '', newPath);
      checkAuth();
    }
  }, [currentView, checkAuth]);

  // Handle Browser Back/Forward navigation
  useEffect(() => {
    const handlePopState = () => {
      setCurrentView(getInitialView());
      checkAuth();
    };
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, [checkAuth]);

  // 3. Dynamic Breadcrumbs (Scoped accurately per context)
  const breadcrumbItems = useMemo(() => {
    const cap = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : '');

    if (currentView.startsWith('sandbox__')) {
      const parts = currentView.split('__');
      const id = parts[1];
      const viewName = parts[2];
      const items = [
        {
          label: `Sandbox (${id.substring(0, 8)}...)`,
          view: `sandbox__${id}__dashboard`,
        },
      ];
      if (viewName && viewName !== 'dashboard') {
        items.push({ label: cap(viewName), view: currentView });
      }
      return items;
    } else if (currentView.startsWith('tenant__')) {
      const parts = currentView.split('__');
      const id = parts[1];
      const viewName = parts[2];
      const items = [{ label: `Tenant (${id})`, view: `tenant__${id}__dashboard` }];
      if (viewName && viewName !== 'dashboard') {
        items.push({ label: cap(viewName), view: currentView });
      }
      return items;
    }

    const items = [{ label: 'Home', view: 'dashboard' }];
    if (currentView !== 'dashboard') {
      const parts = currentView.split('-');
      const label = parts.map(cap).join(' ');
      items.push({ label: label, view: currentView });
    }

    return items;
  }, [currentView]);

  const sandboxId = currentView.startsWith('sandbox__') ? currentView.split('__')[1] : null;

  if (isLoading)
    return (
      <div className="flex h-screen items-center justify-center bg-background text-primary">
        <Loader2 className="animate-spin w-8 h-8" />
      </div>
    );
  if (!user) return <LoginPage />;

  return (
    <div className="flex min-h-screen bg-background text-foreground font-sans">
      <Sidebar currentView={currentView} onChangeView={setCurrentView} />
      {sandboxId && <SandboxAiToolbar sessionId={sandboxId} />}
      <div className="flex-1 flex flex-col h-screen overflow-hidden">
        <Topbar />
        <main className="flex-1 overflow-y-auto bg-secondary/20 relative">
          <div className="px-6 py-4 sticky top-0 z-20 bg-background/50 backdrop-blur-sm border-b border-border/50">
            <Breadcrumb items={breadcrumbItems as any} onNavigate={setCurrentView} />
          </div>
          <div className="px-6 py-6">
            <Router view={currentView} onChangeView={setCurrentView} />
          </div>
        </main>
      </div>
    </div>
  );
};
