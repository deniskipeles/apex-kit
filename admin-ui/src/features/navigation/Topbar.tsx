import React, { useState } from 'react';
import { Menu, Search, Bell, Sun, Moon, RefreshCw } from 'lucide-react'; // Add RefreshCw
import { Input, Button } from '../../components/form/FormPrimitives';
import { useAuth } from '../../hooks/useAuth';
import { useTheme } from '../../context/ThemeContext';
import { useUiStore } from '../../store/useUiStore';
import { apiClient, pb } from '../../lib/apiClient'; // Import api
import { useToast } from '../../components/feedback/Toast';

interface TopbarProps {
  onMenuClick?: () => void;
}

export const Topbar = ({ onMenuClick }: TopbarProps) => {
  const { user } = useAuth();
  const { theme, setTheme } = useTheme();
  const { toggleSidebar } = useUiStore();
  const { toast } = useToast();
  const [isReloading, setIsReloading] = useState(false);

  const handleMenuClick = onMenuClick || toggleSidebar;

  const handleSystemReload = async () => {
      setIsReloading(true);
      try {
          const res = await pb.admins.reloadSystem(null);
          toast(res.message || 'System reloaded successfully (Schema & Cron)', 'success');
      } catch (e) {
          toast('Failed to reload system \n'+(e.message || ''), 'error');
          console.error(e);
      } finally {
          setIsReloading(false);
      }
  };

  return (
    <header className="sticky top-0 z-30 flex h-16 items-center gap-3 border-b bg-background/80 px-4 backdrop-blur">
      <Button variant="ghost" size="icon" className="md:hidden shrink-0" onClick={handleMenuClick}><Menu className="h-5 w-5" /></Button>
      
      <div className="relative flex-1 md:max-w-md hidden md:block">
        <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
        <Input placeholder="Search..." className="pl-9 border-none bg-secondary/50" />
      </div>
      
      <div className="md:hidden flex-1"></div>

      <div className="flex items-center gap-1 sm:gap-2 ml-auto sm:ml-0">
        {/* RESTART BUTTON */}
        <Button 
            variant="outline" 
            size="sm" 
            className="hidden sm:flex gap-2 text-xs h-9" 
            onClick={handleSystemReload}
        >
            <RefreshCw className={`h-3.5 w-3.5 ${isReloading ? 'animate-spin' : ''}`} />
            <span className="hidden lg:inline">Restart App</span>
        </Button>

        <Button variant="ghost" size="icon" onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>
          {theme === 'dark' ? <Sun className="h-5 w-5" /> : <Moon className="h-5 w-5" />}
        </Button>
        <Button variant="ghost" size="icon"><Bell className="h-5 w-5" /></Button>
        <div className="h-8 w-8 rounded-full bg-primary/20 flex items-center justify-center text-xs font-bold border border-primary/10 text-primary">
            {user?.email?.[0].toUpperCase()}
        </div>
      </div>
    </header>
  );
};