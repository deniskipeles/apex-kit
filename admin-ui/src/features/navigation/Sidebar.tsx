import React from 'react';
import { LayoutDashboard, Database, Table, Settings, LogOut, Folder, Activity, Sparkles, Users, FileCode, BrainCircuit, LayoutTemplate } from 'lucide-react';
import { ViewState } from '../../types';
import { Button } from '../../components/form/FormPrimitives';
import { useAuth } from '../../hooks/useAuth';
import { useUiStore } from '../../store/useUiStore';

interface SidebarProps {
  currentView: ViewState;
  onChangeView: (view: ViewState) => void;
  isOpen?: boolean;
  onClose?: () => void;
}

export const Sidebar = ({ currentView, onChangeView, isOpen, onClose }: SidebarProps) => {
  const { logout } = useAuth();
  const { isSidebarOpen: storeIsOpen, closeSidebar: storeClose } = useUiStore();

  const showSidebar = isOpen !== undefined ? isOpen : storeIsOpen;
  const handleClose = onClose || storeClose;

  const links: { icon: any, label: string, view: ViewState }[] = [
    { icon: LayoutDashboard, label: 'Dashboard', view: 'dashboard' },
    { icon: Database, label: 'Collections', view: 'collections' },
    { icon: Table, label: 'Records', view: 'records' },
    { icon: Users, label: 'Users', view: 'users' },
    { icon: Folder, label: 'Files', view: 'files' },
    { icon: Activity, label: 'Logs', view: 'logs' },
    { icon: Settings, label: 'Settings', view: 'settings' },
    { icon: Sparkles, label: 'Architect', view: 'ai-architect' },
    { icon: FileCode, label: 'Scripts', view: 'scripts' },
    { icon: LayoutTemplate, label: 'Templates', view: 'templates' },
    { icon: BrainCircuit, label: 'AI Actions', view: 'ai-actions' },
  ];

  return (
    <>
       {showSidebar && <div className="fixed inset-0 bg-black/50 z-40 md:hidden" onClick={handleClose} />}
       <div className={`fixed inset-y-0 left-0 z-50 flex w-64 flex-col border-r border-border bg-background transition-transform duration-300 md:static md:translate-x-0 ${showSidebar ? 'translate-x-0' : '-translate-x-full'}`}>
        <div className="flex h-16 shrink-0 items-center px-6 border-b">
           <img src="src/assets/images/tinybase-logo.svg" alt="Tinybase Logo" className="h-6 w-auto text-primary" />
        </div>
        <div className="flex-1 overflow-y-auto py-4 px-3 space-y-1">
            {links.map(link => (
            <button key={link.view} onClick={() => { onChangeView(link.view); handleClose(); }} className={`w-full flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors ${currentView === link.view ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-secondary'}`}>
                <link.icon className="h-4 w-4" /> {link.label}
            </button>
            ))}
        </div>
        <div className="p-4 border-t">
            <Button variant="ghost" className="w-full justify-start gap-2" onClick={logout}><LogOut className="h-4 w-4" /> Sign Out</Button>
        </div>
        </div>
    </>
  );
};
