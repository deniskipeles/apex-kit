import React, { useState, useEffect, useMemo } from 'react';
import { Loader2 } from 'lucide-react';
import { useAuth } from '../../hooks/useAuth';
import { LoginPage } from '../../features/auth/pages/LoginPage';
import { Sidebar } from '../../features/navigation/Sidebar';
import { Topbar } from '../../features/navigation/Topbar';
import { Breadcrumb } from '../../components/navigation/Breadcrumb';
import { Router } from '../routes';
import { SandboxAiToolbar } from '../../features/ai/components/SandboxAiToolbar';

export const MainLayout = () => {
  const { user, isLoading, checkAuth } = useAuth(); // <--- Destructure checkAuth

  // 1. Initialize View State from URL
  const getInitialView = () => {
    const path = window.location.pathname;

    const tenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)\/?(.*)/);
    if (tenantMatch) {
      const subPath = tenantMatch[2] || 'dashboard';
      return `tenant__${tenantMatch[1]}__${subPath}`;
    }

    const sandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)\/?(.*)/);
    if (sandboxMatch) {
      const subPath = sandboxMatch[2] || 'dashboard';
      return `sandbox__${sandboxMatch[1]}__${subPath}`;
    }

    const dashboardMatch = path.match(/^\/_dashboard\/?(.*)/);
    return dashboardMatch && dashboardMatch[1] ? dashboardMatch[1] : 'dashboard';
  };

  const [currentView, setCurrentView] = useState(getInitialView());

  // 2. Sync URL with State Changes
  useEffect(() => {
    const path = window.location.pathname;
    let newPath = '/_dashboard';

    if (currentView.startsWith('tenant__')) {
      const parts = currentView.split('__');
      if (parts.length >= 3) {
        newPath = `/_dashboard/tenant/${parts[1]}/${parts[2]}`;
      }
    } else if (currentView.startsWith('sandbox__')) {
      const parts = currentView.split('__');
      if (parts.length >= 3) {
        newPath = `/_dashboard/sandbox/${parts[1]}/${parts[2]}`;
      }
    } else {
      newPath = `/_dashboard/${currentView === 'dashboard' ? '' : currentView}`;
    }

    if (path !== newPath) {
      window.history.pushState({}, '', newPath);
      // 2b. Re-check Auth when context switches (e.g. root -> tenant)
      checkAuth();
    }
  }, [currentView, checkAuth]);

  // Handle Browser Back/Forward
  useEffect(() => {
    const handlePopState = () => {
      setCurrentView(getInitialView());
      checkAuth(); // Re-check on history navigation
    };
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, [checkAuth]);

  // 3. Dynamic Breadcrumbs
  const breadcrumbItems = useMemo(() => {
    const items = [{ label: 'Home', view: 'dashboard' }];
    const cap = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : '');

    if (currentView.startsWith('sandbox__')) {
      const parts = currentView.split('__');
      const id = parts[1];
      const viewName = parts[2];
      items.push({
        label: `Sandbox (${id.substring(0, 8)}...)`,
        view: `sandbox__${id}__dashboard`,
      });
      if (viewName && viewName !== 'dashboard') {
        items.push({ label: cap(viewName), view: currentView });
      }
    } else if (currentView.startsWith('tenant__')) {
      const parts = currentView.split('__');
      const id = parts[1];
      const viewName = parts[2];
      items.push({ label: `Tenant (${id})`, view: `tenant__${id}__dashboard` });
      if (viewName && viewName !== 'dashboard') {
        items.push({ label: cap(viewName), view: currentView });
      }
    } else if (currentView !== 'dashboard') {
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
