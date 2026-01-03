import React, { useState } from 'react';
import { Archive, Clock, Play, Trash2, Plus, Save } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Switch, Button, Badge, Select } from '../../../components/ui/Elements';
import { AppSettings, CronJob } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';

interface BackupSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const BackupSettings = ({ settings, onChange, onSave }: BackupSettingsProps) => {
  const { toast } = useToast();
  const [newCronCmd, setNewCronCmd] = useState('');
  const [newCronSchedule, setNewCronSchedule] = useState('0 0 * * *');
  const [isSaving, setIsSaving] = useState(false);

  const updateBackup = (updates: any) => {
      onChange({ backups: { ...settings.backups, ...updates } });
  };

  const addCronJob = async () => {
    if(!newCronCmd) return;
    const newJob: CronJob = {
        id: `cron_${Math.random().toString(36).substr(2, 6)}`,
        name: 'New Task',
        schedule: newCronSchedule,
        payload: newCronCmd,
        active: true
    };
    
    const updatedJobs = [...settings.cronJobs, newJob];
    onChange({ cronJobs: updatedJobs });
    setNewCronCmd('');
    toast('Cron job added to list (click Save to persist)', 'info');
  };

  const removeCronJob = (id: string) => {
      onChange({ cronJobs: settings.cronJobs.filter(c => c.id !== id) });
  };

  const toggleCronJob = (id: string) => {
      onChange({ cronJobs: settings.cronJobs.map(c => c.id === id ? { ...c, active: !c.active } : c) });
  };

  const handleSaveClick = async () => {
      setIsSaving(true);
      try {
          await onSave({ backups: settings.backups, cronJobs: settings.cronJobs });
      } finally {
          setIsSaving(false);
      }
  };

  return (
    <div className="space-y-6">
        <Card>
            <CardHeader>
                <div className="flex items-center justify-between">
                    <CardTitle className="flex items-center gap-2"><Archive className="h-4 w-4" /> Database Backups</CardTitle>
                    <Switch checked={settings.backups.enabled} onCheckedChange={(c: boolean) => updateBackup({ enabled: c })} />
                </div>
            </CardHeader>
            <CardContent className="space-y-6">
                 <div className={`grid grid-cols-1 md:grid-cols-3 gap-6 transition-opacity ${settings.backups.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}>
                     <div className="space-y-2"><Label>Schedule (Cron)</Label><Input value={settings.backups.schedule} onChange={(e: any) => updateBackup({ schedule: e.target.value })} className="font-mono" /><p className="text-[10px] text-muted-foreground">Current: Daily at 00:00</p></div>
                     <div className="space-y-2"><Label>Retention (Days)</Label><Input type="number" value={settings.backups.retention} onChange={(e: any) => updateBackup({ retention: Number(e.target.value) })} /></div>
                     <div className="space-y-2"><Label>Destination</Label><Select value={settings.backups.destination} onChange={(e: any) => updateBackup({ destination: e.target.value })}><option value="local">Local Storage</option><option value="s3">S3 Storage</option></Select></div>
                 </div>
                 <div className="border-t pt-4 flex justify-end"><Button size="sm" onClick={() => toast('Backup started manually...', 'info')}><Play className="mr-2 h-3 w-3" /> Run Backup Now</Button></div>
            </CardContent>
        </Card>

        <Card>
            <CardHeader><CardTitle className="flex items-center gap-2"><Clock className="h-4 w-4" /> Cron Jobs</CardTitle></CardHeader>
            <CardContent className="space-y-4">
                <div className="space-y-4">
                    {settings.cronJobs.map(job => (
                        <div key={job.id} className="flex items-center gap-4 p-3 rounded-lg border border-border bg-card">
                            <div className="flex-1 grid grid-cols-1 md:grid-cols-3 gap-4 items-center">
                                <div><div className="font-medium text-sm">{job.name}</div><div className="text-xs text-muted-foreground font-mono">{job.payload}</div></div>
                                <div className="flex items-center gap-2"><Badge variant="secondary" className="font-mono text-[10px]">{job.schedule}</Badge></div>
                                <div className="flex items-center gap-2 justify-end"><Switch checked={job.active} onCheckedChange={() => toggleCronJob(job.id)} /><Button size="icon" variant="ghost" className="h-8 w-8 text-destructive" onClick={() => removeCronJob(job.id)}><Trash2 className="h-4 w-4" /></Button></div>
                            </div>
                        </div>
                    ))}
                </div>
                <div className="flex flex-col sm:flex-row gap-2 pt-4 border-t">
                     <Input placeholder="Command / Payload" value={newCronCmd} onChange={(e: any) => setNewCronCmd(e.target.value)} className="flex-1 font-mono text-sm" />
                     <Input placeholder="Schedule" value={newCronSchedule} onChange={(e: any) => setNewCronSchedule(e.target.value)} className="w-full sm:w-32 font-mono text-sm" />
                     <Button onClick={addCronJob}><Plus className="mr-2 h-4 w-4" /> Add Job</Button>
                </div>
            </CardContent>
        </Card>

        <div className="flex justify-end">
            <Button onClick={handleSaveClick} isLoading={isSaving} className="w-full sm:w-auto">
                <Save className="mr-2 h-4 w-4" /> Save System Settings
            </Button>
        </div>
    </div>
  );
};