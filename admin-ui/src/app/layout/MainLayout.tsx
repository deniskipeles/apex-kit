
import React, { useState, useMemo } from 'react';
import { Loader2 } from 'lucide-react';
import { useAuth } from '../../hooks/useAuth';
import { LoginPage } from '../../features/auth/pages/LoginPage'; 
import { Sidebar } from '../../features/navigation/Sidebar';
import { Topbar } from '../../features/navigation/Topbar';
import { Breadcrumb } from '../../components/navigation/Breadcrumb';
import { Router } from '../routes';
import { ViewState } from '../../types';

export const MainLayout = () => {
  const { user, isLoading } = useAuth();
  const [currentView, setCurrentView] = useState<ViewState>('dashboard');

  const breadcrumbItems = useMemo(() => {
    const items = [{ label: 'Dashboard', view: 'dashboard' as ViewState }];
    if (currentView !== 'dashboard') {
        const parts = currentView.split('-');
        const label = parts[0].charAt(0).toUpperCase() + parts[0].slice(1);
        items.push({ label, view: currentView });
        if(parts[1]) {
             items.push({ label: parts[1].charAt(0).toUpperCase() + parts[1].slice(1), view: currentView });
        }
    }
    return items;
  }, [currentView]);

  if (isLoading) return <div className="flex h-screen items-center justify-center"><Loader2 className="animate-spin" /></div>;
  if (!user) return <LoginPage />;

  return (
    <div className="flex min-h-screen bg-background text-foreground font-sans">
      <Sidebar 
        currentView={currentView} 
        onChangeView={setCurrentView} 
      />
      <div className="flex-1 flex flex-col h-screen overflow-hidden">
        <Topbar />
        <main className="flex-1 overflow-y-auto bg-secondary/20">
          <div className="px-6 py-4">
             <Breadcrumb items={breadcrumbItems} onNavigate={setCurrentView} />
          </div>
          <div className="px-6 pb-6">
            <Router view={currentView} onChangeView={setCurrentView} />
          </div>
        </main>
      </div>
    </div>
  );
};