
import React, { useEffect, useState } from 'react';
import { Save, Loader2, Settings as SettingsIcon, Shield, Database, BrainCircuit, HardDrive, Key } from 'lucide-react';
import { Button } from '../../../components/ui/Elements';
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

  if (isLoading || !settings) return <div className="flex justify-center p-12"><Loader2 className="animate-spin h-8 w-8 text-primary" /></div>;

  const tabs: { id: Tab; label: string; icon: any }[] = [
      { id: 'general', label: 'General', icon: SettingsIcon },
      { id: 'security', label: 'Security', icon: Shield },
      { id: 'storage', label: 'Storage', icon: HardDrive },
      { id: 'ai', label: 'AI', icon: BrainCircuit },
      { id: 'backups', label: 'System & Backups', icon: Database },
      { id: 'tokens', label: 'API Keys', icon: Key },
  ];

  return (
    <div className="space-y-6 pb-20">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
            <div>
                <h2 className="text-3xl font-bold tracking-tight">Settings</h2>
                <p className="text-muted-foreground">Manage application configuration.</p>
            </div>
            <Button onClick={handleSave} isLoading={isSaving}>
                <Save className="mr-2 h-4 w-4" /> Save Changes
            </Button>
        </div>

        <div className="flex space-x-1 rounded-xl bg-secondary/20 p-1 overflow-x-auto">
            {tabs.map(tab => (
                <button 
                    key={tab.id}
                    className={`flex items-center gap-2 whitespace-nowrap px-4 rounded-lg py-2.5 text-sm font-medium leading-5 transition-all ${activeTab === tab.id ? 'bg-background shadow text-primary' : 'text-muted-foreground hover:bg-white/[0.12] hover:text-white'}`}
                    onClick={() => setActiveTab(tab.id)}
                >
                    <tab.icon className="h-4 w-4" />
                    {tab.label}
                </button>
            ))}
        </div>

        <div className="animate-in fade-in slide-in-from-bottom-4 duration-300">
            {activeTab === 'general' && <GeneralSettings settings={settings} onChange={updateLocalSettings} />}
            {activeTab === 'security' && <SecuritySettings settings={settings} onChange={updateLocalSettings} />}
            {activeTab === 'storage' && <StorageSettings settings={settings} onChange={updateLocalSettings} />}
            {activeTab === 'backups' && <BackupSettings settings={settings} onChange={updateLocalSettings} />}
            {activeTab === 'tokens' && <TokenSettings settings={settings} onChange={updateLocalSettings} />}
            {activeTab === 'ai' && <AiSettings settings={settings} onChange={updateLocalSettings} />}
        </div>
    </div>
  );
};
