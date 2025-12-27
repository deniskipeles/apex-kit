import React from 'react';
import { 
    LayoutDashboard, Database, Table, Settings, LogOut, Folder, 
    Activity, Sparkles, Users, FileCode, BrainCircuit, LayoutTemplate, 
    Server, ArrowLeft 
} from 'lucide-react';
import { Button } from '../../components/form/FormPrimitives';
import { useAuth } from '../../hooks/useAuth';
import { useUiStore } from '../../store/useUiStore';
import { apiClient } from '@/src/lib/apiClient';

interface SidebarProps {
  currentView: string;
  onChangeView: (view: string) => void;
  isOpen?: boolean;
  onClose?: () => void;
}

export const Sidebar = ({ currentView, onChangeView, isOpen, onClose }: SidebarProps) => {
  const { logout } = useAuth();
  const { isSidebarOpen: storeIsOpen, closeSidebar: storeClose } = useUiStore();

  const showSidebar = isOpen !== undefined ? isOpen : storeIsOpen;
  const handleClose = onClose || storeClose;

  // 1. Detect Context (Root vs Tenant vs Sandbox)
  let contextType: 'root' | 'tenant' | 'sandbox' = 'root';
  let contextId = '';

  // FIX: Match double underscore logic
  if (currentView.startsWith('tenant__')) {
      contextType = 'tenant';
      contextId = currentView.split('__')[1];
  } else if (currentView.startsWith('sandbox__')) {
      contextType = 'sandbox';
      contextId = currentView.split('__')[1];
  }

  // 2. Define Links
  const links = [
    { icon: LayoutDashboard, label: 'Dashboard', page: 'dashboard' },
    { icon: Database, label: 'Collections', page: 'collections' },
    { icon: Table, label: 'Records', page: 'records' },
    { icon: Users, label: 'Users', page: 'users' },
    { icon: Folder, label: 'Files', page: 'files' },
    { icon: FileCode, label: 'Scripts', page: 'scripts' },
    { icon: LayoutTemplate, label: 'Templates', page: 'templates' },
    { icon: BrainCircuit, label: 'Vector Search', page: 'vector-search' },
    { icon: Activity, label: 'Logs', page: 'logs' },
    { icon: Settings, label: 'Settings', page: 'settings' },
  ];
  
  // Root Only Links
  if (contextType === 'root') {
      // Insert Tenants after Dashboard
      links.splice(1, 0, { icon: Server, label: 'Tenants', page: 'tenants' });
      // Append System features
      links.splice(7, 0, { icon: BrainCircuit, label: 'AI Actions', page: 'ai-actions' });
      links.splice(6, 0, { icon: Sparkles, label: 'AI Architect', page: 'ai-architect' });
  }

  // Helper: Construct target view string based on context
  const getTargetView = (page: string) => {
    if (contextType === 'tenant') return `tenant__${contextId}__${page}`;
    if (contextType === 'sandbox') return `sandbox__${contextId}__${page}`;
    return page;
  };

  return (
    <>
       {showSidebar && <div className="fixed inset-0 bg-black/50 z-40 md:hidden" onClick={handleClose} />}
       <div className={`fixed inset-y-0 left-0 z-50 flex w-64 flex-col border-r border-border bg-background transition-transform duration-300 md:static md:translate-x-0 ${showSidebar ? 'translate-x-0' : '-translate-x-full'}`}>
        
        {/* Header Section */}
        <div className="flex h-16 shrink-0 items-center px-6 border-b">
           {contextType === 'root' ? (
               <div className="flex items-center gap-2">
                   <img src={apiClient.apiUrl+"/logo?thumb=100x100"} alt="Tinybase" className="h-6 w-auto text-primary" />
                   <span className="font-bold text-lg tracking-tight">TinyBase</span>
               </div>
           ) : (
               <div className="flex flex-col w-full justify-center h-full">
                   <div className="flex items-center justify-between mb-1">
                       <span className="text-[10px] font-bold uppercase text-muted-foreground tracking-wider">
                           {contextType === 'sandbox' ? 'Sandbox Mode' : 'Tenant Mode'}
                       </span>
                   </div>
                   <div className="flex items-center gap-2 overflow-hidden">
                       <div className={`w-2 h-2 rounded-full shrink-0 ${contextType === 'sandbox' ? 'bg-amber-500' : 'bg-blue-500'}`}></div>
                       <span className={`font-mono text-xs truncate w-full font-medium ${contextType === 'sandbox' ? 'text-amber-500' : 'text-blue-500'}`}>
                           {contextId}
                       </span>
                   </div>
               </div>
           )}
        </div>

        {/* Navigation Links */}
        <div className="flex-1 overflow-y-auto py-4 px-3 space-y-1">
            {/* Exit Context Button */}
            {contextType !== 'root' && (
                <button 
                    onClick={() => { window.location.href = '/_dashboard'; }} 
                    className="w-full flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground hover:bg-destructive/10 hover:text-destructive mb-6 border border-dashed border-border transition-colors group"
                >
                    <ArrowLeft className="h-4 w-4 group-hover:-translate-x-1 transition-transform" /> 
                    Exit {contextType === 'sandbox' ? 'Sandbox' : 'Tenant'}
                </button>
            )}

            {links.map(link => {
                const targetView = getTargetView(link.page);
                // Active state check handles nested views (e.g. collections-create)
                const isActive = currentView === targetView || currentView.startsWith(targetView + '-');
                
                return (
                    <button 
                        key={link.page} 
                        onClick={() => { onChangeView(targetView); handleClose(); }} 
                        className={`w-full flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                            isActive 
                                ? 'bg-primary/10 text-primary' 
                                : 'text-muted-foreground hover:bg-secondary hover:text-foreground'
                        }`}
                    >
                        <link.icon className={`h-4 w-4 ${isActive ? 'text-primary' : 'opacity-70'}`} /> 
                        {link.label}
                    </button>
                );
            })}
        </div>
        
        {/* Footer */}
        <div className="p-4 border-t border-border">
            <Button variant="ghost" className="w-full justify-start gap-2 text-muted-foreground hover:text-foreground" onClick={logout}>
                <LogOut className="h-4 w-4" /> Sign Out
            </Button>
        </div>
      </div>
    </>
  );
};