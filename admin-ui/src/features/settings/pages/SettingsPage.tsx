import React, { useEffect, useState } from 'react';
import { 
    Save, Loader2, Settings as SettingsIcon, Shield, Database, 
    BrainCircuit, HardDrive, Key, BellRing, Monitor
} from 'lucide-react';
import { Button, Card, CardHeader, CardTitle, CardContent } from '../../../components/ui/Elements';
import { GeneralSettings } from '../components/GeneralSettings';
import { SecuritySettings } from '../components/SecuritySettings';
import { StorageSettings } from '../components/StorageSettings';
import { BackupSettings } from '../components/BackupSettings';
import { TokenSettings } from '../components/TokenSettings';
import { AiSettings } from '../components/AiSettings';
import { settingsService } from '../services/settingsService';
import { AppSettings } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';

type Tab = 'general' | 'security' | 'storage' | 'backups' | 'tokens' | 'ai';

export const SettingsPage = () => {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<Tab>('general');
  const { toast } = useToast();

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
      try {
          const data = await settingsService.get();
          setSettings(data);
      } catch (e) {
          toast('Failed to load settings', 'error');
      } finally {
          setIsLoading(false);
      }
  };

  const handleSave = async () => {
      if (!settings) return;
      setIsSaving(true);
      try {
          await settingsService.update(settings);
          toast('Settings saved successfully', 'success');
      } catch (e) {
          toast('Failed to save settings', 'error');
      } finally {
          setIsSaving(false);
      }
  };

  const updateLocalSettings = (updates: Partial<AppSettings>) => {
      setSettings(prev => prev ? ({ ...prev, ...updates }) : null);
  };

  const tabs: { id: Tab; label: string; icon: any; desc: string }[] = [
      { id: 'general', label: 'General', icon: SettingsIcon, desc: 'App identity and branding' },
      { id: 'security', label: 'Security', icon: Shield, desc: 'Access control and CORS' },
      { id: 'storage', label: 'Storage', icon: HardDrive, desc: 'File uploads and S3' },
      { id: 'ai', label: 'AI Engine', icon: BrainCircuit, desc: 'LLM providers and keys' },
      { id: 'backups', label: 'System', icon: Database, desc: 'Backups and cron jobs' },
      { id: 'tokens', label: 'API Keys', icon: Key, desc: 'Manage access tokens' },
  ];

  if (isLoading || !settings) {
      return (
          <div className="flex h-[calc(100vh-100px)] items-center justify-center">
              <Loader2 className="animate-spin h-8 w-8 text-primary/50" />
          </div>
      );
  }

  return (
    <div className="flex flex-col h-full max-w-7xl mx-auto space-y-6 pb-20">
        
        {/* Page Header */}
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 border-b border-border/50 pb-6">
            <div>
                <h1 className="text-3xl font-bold tracking-tight flex items-center gap-2">
                    <SettingsIcon className="h-8 w-8 text-primary" /> Settings
                </h1>
                <p className="text-muted-foreground mt-1">Manage your application configuration and preferences.</p>
            </div>
            <Button onClick={handleSave} isLoading={isSaving} className="shadow-lg hover:shadow-primary/20 transition-all">
                <Save className="mr-2 h-4 w-4" /> Save Changes
            </Button>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-start">
            
            {/* Sidebar Navigation */}
            <div className="lg:col-span-3 space-y-1 overflow-x-auto lg:overflow-visible flex lg:flex-col gap-1 lg:gap-1 pb-2 lg:pb-0 sticky top-4">
                {tabs.map(tab => {
                    const isActive = activeTab === tab.id;
                    const Icon = tab.icon;
                    return (
                        <button 
                            key={tab.id}
                            onClick={() => setActiveTab(tab.id)}
                            className={`
                                flex items-center gap-3 px-4 py-3 rounded-lg text-sm font-medium transition-all w-full text-left whitespace-nowrap lg:whitespace-normal
                                ${isActive 
                                    ? 'bg-primary text-primary-foreground shadow-md' 
                                    : 'text-muted-foreground hover:bg-secondary/50 hover:text-foreground'
                                }
                            `}
                        >
                            <Icon className={`h-5 w-5 shrink-0 ${isActive ? 'text-primary-foreground' : 'text-muted-foreground/70'}`} />
                            <div className="flex flex-col">
                                <span>{tab.label}</span>
                                {isActive && <span className="text-[10px] opacity-90 font-normal hidden lg:block">{tab.desc}</span>}
                            </div>
                        </button>
                    );
                })}
            </div>

            {/* Main Content Area */}
            <div className="lg:col-span-9 space-y-6 min-h-[500px]">
                {/* Mobile Helper Title */}
                <div className="lg:hidden font-semibold text-lg flex items-center gap-2 mb-2 px-1">
                    {tabs.find(t => t.id === activeTab)?.icon && React.createElement(tabs.find(t => t.id === activeTab)!.icon, { className: "h-5 w-5 text-primary" })}
                    {tabs.find(t => t.id === activeTab)?.label}
                </div>

                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300">
                    {activeTab === 'general' && <GeneralSettings settings={settings} onChange={updateLocalSettings} />}
                    {activeTab === 'security' && <SecuritySettings settings={settings} onChange={updateLocalSettings} />}
                    {activeTab === 'storage' && <StorageSettings settings={settings} onChange={updateLocalSettings} />}
                    {activeTab === 'backups' && <BackupSettings settings={settings} onChange={updateLocalSettings} />}
                    {activeTab === 'tokens' && <TokenSettings settings={settings} onChange={updateLocalSettings} />}
                    {activeTab === 'ai' && <AiSettings settings={settings} onChange={updateLocalSettings} />}
                </div>
            </div>

        </div>
    </div>
  );
};