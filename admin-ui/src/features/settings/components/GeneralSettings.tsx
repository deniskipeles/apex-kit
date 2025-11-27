
import React from 'react';
import { Monitor, Moon, Sun } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Select, Button } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { useTheme } from '../../../context/ThemeContext';

interface GeneralSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
}

export const GeneralSettings = ({ settings, onChange }: GeneralSettingsProps) => {
  const { setTheme } = useTheme();

  const handleThemeChange = (t: 'light' | 'dark' | 'system') => {
      onChange({ theme: t });
      setTheme(t);
  };

  return (
    <Card>
        <CardHeader>
            <CardTitle>Application Settings</CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                <div className="space-y-2">
                    <Label>Application Name</Label>
                    <Input 
                        value={settings.appName} 
                        onChange={(e: any) => onChange({ appName: e.target.value })} 
                    />
                </div>
                <div className="space-y-2">
                    <Label>Application URL</Label>
                    <Input 
                        value={settings.appUrl} 
                        onChange={(e: any) => onChange({ appUrl: e.target.value })} 
                    />
                </div>
                <div className="space-y-2">
                    <Label>Log Retention (Days)</Label>
                    <Input 
                        type="number" 
                        value={settings.logRetentionDays || 7} 
                        onChange={(e: any) => onChange({ logRetentionDays: Number(e.target.value) })} 
                    />
                    <p className="text-[10px] text-muted-foreground">Older log files on disk will be deleted automatically.</p>
                </div>
            </div>

            <div className="space-y-2">
                <Label>Appearance</Label>
                <div className="grid grid-cols-3 gap-2 sm:gap-4">
                    <Button 
                        variant={settings.theme === 'light' ? 'primary' : 'outline'} 
                        className="h-16 sm:h-20 flex flex-col gap-2"
                        onClick={() => handleThemeChange('light')}
                    >
                        <Sun className="h-5 w-5 sm:h-6 sm:w-6" />
                        <span className="text-xs sm:text-sm">Light</span>
                    </Button>
                    <Button 
                        variant={settings.theme === 'dark' ? 'primary' : 'outline'} 
                        className="h-16 sm:h-20 flex flex-col gap-2"
                        onClick={() => handleThemeChange('dark')}
                    >
                        <Moon className="h-5 w-5 sm:h-6 sm:w-6" />
                        <span className="text-xs sm:text-sm">Dark</span>
                    </Button>
                    <Button 
                        variant={settings.theme === 'system' ? 'primary' : 'outline'} 
                        className="h-16 sm:h-20 flex flex-col gap-2"
                        onClick={() => handleThemeChange('system')}
                    >
                        <Monitor className="h-5 w-5 sm:h-6 sm:w-6" />
                        <span className="text-xs sm:text-sm">System</span>
                    </Button>
                </div>
            </div>
        </CardContent>
    </Card>
  );
};
