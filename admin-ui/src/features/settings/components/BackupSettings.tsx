import React, { useState, useEffect, useRef } from 'react';
import {
  Archive,
  Clock,
  Play,
  Trash2,
  Plus,
  Save,
  Download,
  RotateCcw,
  FileArchive,
  Loader2,
  ShieldAlert,
  Folder,
  Search,
  Database,
  BrainCircuit,
  Globe,
  Upload,
  RefreshCw,
} from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Input,
  Label,
  Switch,
  Button,
  Badge,
  Select,
} from '../../../components/ui/Elements';
import { AppSettings, CronJob } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '@/src/lib/apiClient';
import { formatFileSize } from '@/src/lib/formatters';
import { configService } from '../services/configService';

interface BackupSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
  onSave: (data: Partial<AppSettings>) => Promise<void>;
}

const validateCron = (cron: string) => {
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5 && parts.length !== 6) return false;
  return true;
};

export const BackupSettings = ({ settings, onChange, onSave }: BackupSettingsProps) => {
  const { toast } = useToast();
  const [newCronName, setNewCronName] = useState('');
  const [newCronCmd, setNewCronCmd] = useState('');
  const [newCronSchedule, setNewCronSchedule] = useState('0 0 * * * *');
  const [isSaving, setIsSaving] = useState(false);

  // Backup Management State
  const [backups, setBackups] = useState<any[]>([]);
  const [isLoadingBackups, setIsLoadingBackups] = useState(false);
  const [isRestoring, setIsRestoring] = useState(false);
  const [restoreTarget, setRestoreTarget] = useState<string | null>(null);

  // Upload Backup State
  const [isUploadingRestore, setIsUploadingRestore] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Root Policy State
  const [isRootScope, setIsRootScope] = useState(false);
  const [allowNonRootBackup, setAllowNonRootBackup] = useState(false);

  useEffect(() => {
    const path = window.location.pathname;
    const isRoot =
      !path.includes('/tenant/') &&
      !path.includes('/sandbox/') &&
      apiClient.getScope().type == 'root';
    setIsRootScope(isRoot);

    if (isRoot) {
      configService.list().then((list) => {
        const conf = list.find((c) => c.key === 'ALLOW_NON_ROOT_BACKUP');
        if (conf && conf.value) {
          setAllowNonRootBackup(conf.value === 'true');
        }
      });
    }
  }, []);

  const updateBackup = (updates: any) => {
    onChange({ backups: { ...settings.backups, ...updates } });
  };

  const addCronJob = () => {
    if (!newCronCmd || !newCronName) return;
    if (!validateCron(newCronSchedule)) {
      toast('Invalid Cron Expression', 'error');
      return;
    }

    const newJob: CronJob = {
      id: `cron_${Math.random().toString(36).substr(2, 9)}`,
      name: newCronName,
      schedule: newCronSchedule,
      payload: newCronCmd,
      active: true,
    };

    const updatedJobs = [...(settings.cronJobs || []), newJob];
    onChange({ cronJobs: updatedJobs });

    setNewCronName('');
    setNewCronCmd('');
    toast('Cron job added (pending save)', 'info');
  };

  const removeCronJob = (id: string) => {
    const updated = settings.cronJobs.filter((c) => c.id !== id);
    onChange({ cronJobs: updated });
  };

  const toggleCronJob = (id: string) => {
    const updated = settings.cronJobs.map((c) => (c.id === id ? { ...c, active: !c.active } : c));
    onChange({ cronJobs: updated });
  };

  const handleSaveClick = async () => {
    setIsSaving(true);
    try {
      await onSave({
        backups: settings.backups,
        cronJobs: settings.cronJobs,
      });

      if (isRootScope) {
        await configService.set(
          'ALLOW_NON_ROOT_BACKUP',
          allowNonRootBackup ? 'true' : 'false',
          false
        );
      }

      toast('System settings saved successfully', 'success');
    } catch (e: any) {
      toast(e.message || 'Failed to save settings', 'error');
    } finally {
      setIsSaving(false);
    }
  };

  const loadBackups = async () => {
    setIsLoadingBackups(true);
    try {
      const list = await apiClient.system.listBackups();
      setBackups(list);
    } catch (e) {
      setBackups([]);
    } finally {
      setIsLoadingBackups(false);
    }
  };

  const handleCreateBackup = async () => {
    try {
      await apiClient.system.createBackup();
      toast('Backup job started.', 'success');
      setTimeout(loadBackups, 2000);
    } catch (e: any) {
      toast('Failed to trigger backup', 'error');
    }
  };

  const handleRestore = async (filename: string) => {
    if (!confirm(`Restoring from ${filename} will overwrite current data. Continue?`)) return;
    setIsRestoring(true);
    setRestoreTarget(filename);
    try {
      await apiClient.system.restoreFromFile(filename);
      toast('Restore successful. Reloading...', 'success');
      setTimeout(() => window.location.reload(), 3000);
    } catch (e) {
      toast('Restore failed', 'error');
      setIsRestoring(false);
      setRestoreTarget(null);
    }
  };

  // --- [NEW] Upload Backup Handler ---
  const handleUploadRestore = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    if (!file.name.endsWith('.tar.gz')) {
      toast('Backup file must be a .tar.gz archive', 'error');
      return;
    }

    if (
      !confirm(
        `Uploading and restoring ${file.name} will overwrite your current database and files. Proceed?`
      )
    ) {
      e.target.value = ''; // Reset input
      return;
    }

    setIsUploadingRestore(true);
    toast('Uploading and extracting backup... Please wait.', 'info');

    try {
      await apiClient.system.restoreBackup(file);
      toast('Backup restored successfully. Server is restarting...', 'success');
      // Give the server a few seconds to reboot before refreshing the UI
      setTimeout(() => window.location.reload(), 4000);
    } catch (err: any) {
      console.error(err);
      toast(err.message || 'Failed to upload/restore backup', 'error');
      setIsUploadingRestore(false);
    } finally {
      e.target.value = ''; // Reset input
    }
  };

  const handleDownload = async (filename: string) => {
    try {
      await apiClient.system.downloadBackup(filename);
    } catch (e) {
      toast('Download failed', 'error');
    }
  };

  useEffect(() => {
    if (settings.backups.enabled) {
      loadBackups();
    }
  }, [settings.backups.enabled]);

  return (
    <div className="space-y-6">
      {/* ROOT ONLY: Policy Control */}
      {isRootScope && (
        <Card className="border-amber-500/20 bg-amber-500/5">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-amber-600">
              <ShieldAlert className="h-4 w-4" /> Root Policy
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label>Allow Tenants Local Backups</Label>
                <p className="text-xs text-muted-foreground">
                  If enabled, tenants can save backups to the server disk.
                </p>
              </div>
              <Switch checked={allowNonRootBackup} onCheckedChange={setAllowNonRootBackup} />
            </div>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              <Archive className="h-4 w-4" /> System Backups
            </CardTitle>
            <Switch
              checked={settings.backups.enabled}
              onCheckedChange={(c: boolean) => updateBackup({ enabled: c })}
            />
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          <div
            className={`space-y-6 transition-opacity ${settings.backups.enabled ? 'opacity-100' : 'opacity-50 pointer-events-none'}`}
          >
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              <div className="space-y-2">
                <Label>Schedule (Cron)</Label>
                <Input
                  value={settings.backups.schedule}
                  onChange={(e: any) => updateBackup({ schedule: e.target.value })}
                  className={`font-mono ${!validateCron(settings.backups.schedule) ? 'border-destructive' : ''}`}
                />
              </div>
              <div className="space-y-2">
                <Label>Retention (Days)</Label>
                <Input
                  type="number"
                  value={settings.backups.retention}
                  onChange={(e: any) => updateBackup({ retention: Number(e.target.value) })}
                />
              </div>
              <div className="space-y-2">
                <Label>Destination</Label>
                <Select
                  value={settings.backups.destination}
                  onChange={(e: any) => updateBackup({ destination: e.target.value })}
                >
                  <option value="local">Local Storage</option>
                  <option value="s3">S3 Storage</option>
                </Select>
              </div>
            </div>

            {/* Included Data Options - GRID */}
            <div className="space-y-2">
              <Label>What to include in the backup archive:</Label>
              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 p-4 rounded-lg bg-secondary/10 border border-border">
                {/* Databases */}
                <div className="flex items-center justify-between bg-background p-2.5 rounded border border-border/50 shadow-sm">
                  <div className="space-y-0.5">
                    <Label
                      className="flex items-center gap-2 cursor-pointer text-xs"
                      onClick={() =>
                        updateBackup({ includeDatabases: !settings.backups.includeDatabases })
                      }
                    >
                      <Database className="h-3.5 w-3.5 text-blue-500" /> Databases
                    </Label>
                    <p className="text-[9px] text-muted-foreground">core, data, logs, system</p>
                  </div>
                  <Switch
                    checked={settings.backups.includeDatabases ?? true}
                    onCheckedChange={(c) => updateBackup({ includeDatabases: c })}
                  />
                </div>

                {/* Vectors */}
                <div className="flex items-center justify-between bg-background p-2.5 rounded border border-border/50 shadow-sm">
                  <div className="space-y-0.5">
                    <Label
                      className="flex items-center gap-2 cursor-pointer text-xs"
                      onClick={() =>
                        updateBackup({ includeVectors: !settings.backups.includeVectors })
                      }
                    >
                      <BrainCircuit className="h-3.5 w-3.5 text-purple-500" /> Vector DB
                    </Label>
                    <p className="text-[9px] text-muted-foreground">vectors.db (Can be large)</p>
                  </div>
                  <Switch
                    checked={settings.backups.includeVectors}
                    onCheckedChange={(c) => updateBackup({ includeVectors: c })}
                  />
                </div>

                {/* Static Site */}
                <div className="flex items-center justify-between bg-background p-2.5 rounded border border-border/50 shadow-sm">
                  <div className="space-y-0.5">
                    <Label
                      className="flex items-center gap-2 cursor-pointer text-xs"
                      onClick={() =>
                        updateBackup({ includeStaticSite: !settings.backups.includeStaticSite })
                      }
                    >
                      <Globe className="h-3.5 w-3.5 text-cyan-500" /> Static Site
                    </Label>
                    <p className="text-[9px] text-muted-foreground">The 'public' folder contents</p>
                  </div>
                  <Switch
                    checked={settings.backups.includeStaticSite}
                    onCheckedChange={(c) => updateBackup({ includeStaticSite: c })}
                  />
                </div>

                {/* Uploads */}
                <div className="flex items-center justify-between bg-background p-2.5 rounded border border-border/50 shadow-sm">
                  <div className="space-y-0.5">
                    <Label
                      className="flex items-center gap-2 cursor-pointer text-xs"
                      onClick={() =>
                        updateBackup({ includeUploads: !settings.backups.includeUploads })
                      }
                    >
                      <Folder className="h-3.5 w-3.5 text-orange-500" /> Uploads
                    </Label>
                    <p className="text-[9px] text-muted-foreground">User uploaded files & images</p>
                  </div>
                  <Switch
                    checked={settings.backups.includeUploads}
                    onCheckedChange={(c) => updateBackup({ includeUploads: c })}
                  />
                </div>

                {/* Indexes */}
                <div className="flex items-center justify-between bg-background p-2.5 rounded border border-border/50 shadow-sm">
                  <div className="space-y-0.5">
                    <Label
                      className="flex items-center gap-2 cursor-pointer text-xs"
                      onClick={() =>
                        updateBackup({ includeIndexes: !settings.backups.includeIndexes })
                      }
                    >
                      <Search className="h-3.5 w-3.5 text-pink-500" /> Search Indexes
                    </Label>
                    <p className="text-[9px] text-muted-foreground">
                      Tantivy folders (Can be rebuilt)
                    </p>
                  </div>
                  <Switch
                    checked={settings.backups.includeIndexes}
                    onCheckedChange={(c) => updateBackup({ includeIndexes: c })}
                  />
                </div>
              </div>
            </div>
          </div>

          {/* ... History Table ... */}

          {settings.backups.enabled && (
            <div className="border-t pt-4">
              <div className="flex justify-between items-center mb-4">
                <h4 className="text-sm font-semibold">Backup History</h4>
                <div className="flex gap-2">
                  {/* [NEW] Upload Backup Button */}
                  <input
                    type="file"
                    accept=".tar.gz"
                    ref={fileInputRef}
                    className="hidden"
                    onChange={handleUploadRestore}
                  />
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => fileInputRef.current?.click()}
                    isLoading={isUploadingRestore}
                  >
                    <Upload className="mr-2 h-3 w-3" /> Upload Backup
                  </Button>

                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={loadBackups}
                    isLoading={isLoadingBackups}
                  >
                    <RefreshCw className="h-3 w-3" />
                  </Button>

                  <Button size="sm" onClick={handleCreateBackup}>
                    <Play className="mr-2 h-3 w-3" /> Run Backup Now
                  </Button>
                </div>
              </div>

              <div className="bg-secondary/10 rounded-md border border-border overflow-hidden max-h-[300px] overflow-y-auto">
                {backups.length === 0 ? (
                  <div className="p-8 text-center text-muted-foreground text-sm">
                    No backups found.
                  </div>
                ) : (
                  <div className="divide-y divide-border">
                    {backups.map((b) => (
                      <div
                        key={b.name}
                        className="flex items-center justify-between p-3 hover:bg-secondary/20 transition-colors"
                      >
                        <div className="flex items-center gap-3">
                          <FileArchive className="h-5 w-5 text-primary/70" />
                          <div>
                            <div className="text-sm font-medium">{b.name}</div>
                            <div className="text-xs text-muted-foreground">
                              {new Date(b.created).toLocaleString()} • {formatFileSize(b.size)}
                            </div>
                          </div>
                        </div>
                        <div className="flex gap-1">
                          <Button
                            size="icon"
                            variant="ghost"
                            onClick={() => handleDownload(b.name)}
                            title="Download"
                          >
                            <Download className="h-4 w-4" />
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            onClick={() => handleRestore(b.name)}
                            title="Restore"
                            className="text-amber-500 hover:text-amber-600 hover:bg-amber-500/10"
                          >
                            {isRestoring && restoreTarget === b.name ? (
                              <Loader2 className="h-4 w-4 animate-spin" />
                            ) : (
                              <RotateCcw className="h-4 w-4" />
                            )}
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Cron Jobs */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Clock className="h-4 w-4" /> Cron Jobs
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-4">
            {(settings.cronJobs || []).map((job) => (
              <div
                key={job.id}
                className="flex items-center gap-4 p-3 rounded-lg border border-border bg-card hover:bg-accent/5 transition-colors"
              >
                <div className="flex-1 grid grid-cols-1 md:grid-cols-12 gap-4 items-center">
                  <div className="md:col-span-5">
                    <div className="font-bold text-sm text-foreground">{job.name}</div>
                    <div
                      className="text-xs text-muted-foreground font-mono truncate"
                      title={job.payload}
                    >
                      {job.payload}
                    </div>
                  </div>
                  <div className="md:col-span-4 flex items-center gap-2">
                    <Badge
                      variant="secondary"
                      className="font-mono text-[10px] bg-secondary/50 border-secondary"
                    >
                      {job.schedule}
                    </Badge>
                  </div>
                  <div className="md:col-span-3 flex items-center gap-2 justify-end">
                    <Switch checked={job.active} onCheckedChange={() => toggleCronJob(job.id)} />
                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-8 w-8 text-destructive hover:bg-destructive/10"
                      onClick={() => removeCronJob(job.id)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </div>
            ))}
            {(settings.cronJobs || []).length === 0 && (
              <div className="text-center py-4 text-sm text-muted-foreground italic">
                No active jobs.
              </div>
            )}
          </div>

          {/* Add Job Form */}
          <div className="flex flex-col md:flex-row gap-3 pt-4 border-t items-end">
            <div className="flex-1 w-full space-y-1">
              <Label className="text-xs">Job Name</Label>
              <Input
                placeholder="e.g. Daily Clean"
                value={newCronName}
                onChange={(e: any) => setNewCronName(e.target.value)}
                className="h-9"
              />
            </div>
            <div className="flex-[2] w-full space-y-1">
              <Label className="text-xs">Payload (Script / Webhook)</Label>
              <Input
                placeholder="script-name or /api/..."
                value={newCronCmd}
                onChange={(e: any) => setNewCronCmd(e.target.value)}
                className="h-9 font-mono"
              />
            </div>
            <div className="w-full md:w-32 space-y-1">
              <Label className="text-xs">Schedule</Label>
              <Input
                placeholder="0 * * * * *"
                value={newCronSchedule}
                onChange={(e: any) => setNewCronSchedule(e.target.value)}
                className="h-9 font-mono"
              />
            </div>
            <Button
              onClick={addCronJob}
              disabled={!newCronName || !newCronCmd}
              className="h-9 px-4"
            >
              <Plus className="mr-2 h-4 w-4" /> Add
            </Button>
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
