import React, { useState, useEffect, useRef } from 'react';
import {
  ArrowUpCircle,
  RotateCcw,
  History,
  Upload,
  Link as LinkIcon,
  RefreshCw,
  ShieldCheck,
  AlertTriangle,
  Trash2,
  Cpu,
  CheckCircle2,
  HardDriveDownload,
  Info,
} from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Badge,
  Label,
  Input,
} from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { useToast } from '../../../components/feedback/Toast';
import { formatFileSize } from '../../../lib/formatters';
import { apiClient } from '@/src/lib/apiClient';
import { AppVersions, RollbackItem } from '../../../types';

export const UpgradeSettings = () => {
  const [versions, setVersions] = useState<AppVersions | null>(null);
  const [rollbacks, setRollbacks] = useState<RollbackItem[]>([]);
  const [isLoadingRollbacks, setIsLoadingRollbacks] = useState(true);

  // Upgrade form state
  const [upgradeMode, setUpgradeMode] = useState<'upload' | 'remote'>('upload');
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [remoteUrl, setRemoteUrl] = useState('');
  const [sha256Input, setSha256Input] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Action / Confirmation modal states
  const [confirmUpgradeOpen, setConfirmUpgradeOpen] = useState(false);
  const [rollbackToTarget, setRollbackToTarget] = useState<RollbackItem | null>(null);
  const [rollbackToDelete, setRollbackToDelete] = useState<RollbackItem | null>(null);

  // Reconnection polling state
  const [isHandoverActive, setIsHandoverActive] = useState(false);
  const [handoverMessage, setHandoverMessage] = useState('');
  const [pollAttempt, setPollAttempt] = useState(0);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const { toast } = useToast();

  const loadInitialData = async () => {
    try {
      const ver = await apiClient.getVersions();
      setVersions(ver);
    } catch {
      // Ignore
    }
    loadRollbacks();
  };

  const loadRollbacks = async () => {
    setIsLoadingRollbacks(true);
    try {
      const list = await apiClient.root.listRollbacks();
      setRollbacks(list);
    } catch (e: any) {
      toast(e.message || 'Failed to list rollback binaries', 'error');
    } finally {
      setIsLoadingRollbacks(false);
    }
  };

  useEffect(() => {
    loadInitialData();
  }, []);

  // Handover Polling Loop
  useEffect(() => {
    let timer: NodeJS.Timeout;
    if (isHandoverActive) {
      timer = setInterval(async () => {
        setPollAttempt((prev) => prev + 1);
        try {
          const res = await fetch('/version', { cache: 'no-store' });
          if (res.ok) {
            const newVer: AppVersions = await res.json();
            clearInterval(timer);
            setIsHandoverActive(false);
            setVersions(newVer);
            toast(`Process handover complete! Running Root v${newVer.root}`, 'success');
            setTimeout(() => window.location.reload(), 1500);
          }
        } catch {
          // Keep polling while server reboots
        }
      }, 2000);
    }
    return () => clearInterval(timer);
  }, [isHandoverActive, toast]);

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      setSelectedFile(file);
    }
  };

  const handlePerformUpgrade = async () => {
    setConfirmUpgradeOpen(false);
    setIsSubmitting(true);
    toast('Uploading and verifying binary candidate...', 'info');

    try {
      let res;
      if (upgradeMode === 'upload' && selectedFile) {
        res = await apiClient.root.uploadUpgrade(selectedFile, sha256Input);
      } else if (upgradeMode === 'remote' && remoteUrl) {
        res = await apiClient.root.remoteUpgrade(remoteUrl, sha256Input);
      } else {
        throw new Error('Please specify a binary file or URL.');
      }

      toast(res.message, 'success');
      setHandoverMessage(res.message);
      setIsHandoverActive(true);
      setPollAttempt(0);
      setSelectedFile(null);
      setRemoteUrl('');
      setSha256Input('');
      if (fileInputRef.current) fileInputRef.current.value = '';
    } catch (e: any) {
      toast(e.message || 'Upgrade failed', 'error');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handlePerformRollback = async () => {
    if (!rollbackToTarget) return;
    const target = rollbackToTarget;
    setRollbackToTarget(null);
    setIsSubmitting(true);
    toast(`Restoring binary '${target.name}'...`, 'info');

    try {
      const res = await apiClient.root.rollback(target.name);
      toast(res.message, 'success');
      setHandoverMessage(`Rolling back to previous executable '${target.name}'...`);
      setIsHandoverActive(true);
      setPollAttempt(0);
    } catch (e: any) {
      toast(e.message || 'Rollback failed', 'error');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteRollback = async () => {
    if (!rollbackToDelete) return;
    const target = rollbackToDelete;
    setRollbackToDelete(null);

    try {
      await apiClient.root.deleteRollback(target.name);
      toast(`Deleted rollback '${target.name}'`, 'success');
      loadRollbacks();
    } catch (e: any) {
      toast(e.message || 'Failed to delete rollback', 'error');
    }
  };

  return (
    <div className="space-y-6">
      {/* 1. Header Overview & Safety Policy */}
      <Card className="border-primary/20 bg-gradient-to-br from-primary/5 via-secondary/5 to-background">
        <CardHeader className="pb-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <CardTitle className="flex items-center gap-2 text-primary text-lg">
              <ArrowUpCircle className="h-5 w-5" /> Host Binary Upgrades & Rollbacks
            </CardTitle>
            {versions && (
              <Badge variant="outline" className="font-mono text-xs px-2.5 py-1 bg-background">
                Root Binary: v{versions.root}
              </Badge>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground leading-relaxed">
            ApexKit features an atomic, zero-downtime binary handover mechanism with socket reuse.
            When an upgrade or rollback is triggered, the engine smoke-tests the candidate
            executable, archives the running binary with an index (e.g.{' '}
            <code className="text-xs font-mono bg-secondary/50 px-1 py-0.5 rounded">
              apexkit__old-1
            </code>
            ), swaps the binary on disk, and initiates a seamless process handover while draining
            in-flight requests.
          </p>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-2">
            {[
              { label: 'System Root', ver: versions?.root },
              { label: 'API Layer', ver: versions?.api },
              { label: 'Core Engine', ver: versions?.core },
              { label: 'Vector Engine', ver: versions?.vector },
            ].map((item) => (
              <div
                key={item.label}
                className="p-3 rounded-lg border border-border bg-card/60 space-y-1"
              >
                <div className="text-[10px] uppercase font-bold text-muted-foreground">
                  {item.label}
                </div>
                <div className="text-xs font-mono font-bold text-foreground">
                  {item.ver ? `v${item.ver}` : 'Loading...'}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* 2. Upgrade Workspace */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-base flex items-center gap-2">
              <Cpu className="h-4 w-4 text-emerald-500" /> Apply New Version
            </CardTitle>
            <div className="flex items-center gap-1 bg-secondary/30 p-0.5 rounded-lg border border-border text-xs">
              <button
                type="button"
                onClick={() => setUpgradeMode('upload')}
                className={`px-3 py-1 rounded font-semibold transition-all ${
                  upgradeMode === 'upload'
                    ? 'bg-background shadow-sm text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <Upload className="h-3 w-3 inline mr-1" /> File Upload
              </button>
              <button
                type="button"
                onClick={() => setUpgradeMode('remote')}
                className={`px-3 py-1 rounded font-semibold transition-all ${
                  upgradeMode === 'remote'
                    ? 'bg-background shadow-sm text-foreground'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                <LinkIcon className="h-3 w-3 inline mr-1" /> Remote URL
              </button>
            </div>
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          {upgradeMode === 'upload' ? (
            <div className="space-y-3">
              <div className="space-y-1.5">
                <Label required>Artifact or Executable Binary</Label>
                <div className="flex items-center gap-2">
                  <input
                    type="file"
                    ref={fileInputRef}
                    className="hidden"
                    onChange={handleFileSelect}
                  />
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => fileInputRef.current?.click()}
                    className="w-full sm:w-auto text-xs"
                    disabled={isSubmitting}
                  >
                    <Upload className="mr-2 h-4 w-4" />{' '}
                    {selectedFile ? 'Change Selected File' : 'Select Binary (.tar.gz, .zip, raw)'}
                  </Button>
                  {selectedFile && (
                    <span className="text-xs font-mono text-muted-foreground truncate">
                      {selectedFile.name} ({formatFileSize(selectedFile.size)})
                    </span>
                  )}
                </div>
                <p className="text-[10px] text-muted-foreground">
                  Accepts raw uncompressed binaries or release archives (<code>.tar.gz</code>,{' '}
                  <code>.zip</code>, <code>.tar</code>).
                </p>
              </div>
            </div>
          ) : (
            <div className="space-y-1.5">
              <Label required>Direct Download Artifact URL</Label>
              <Input
                placeholder="https://github.com/org/repo/releases/download/v1.1.0/apexkit-linux-x86_64.tar.gz"
                value={remoteUrl}
                onChange={(e: any) => setRemoteUrl(e.target.value)}
                disabled={isSubmitting}
                className="font-mono text-xs"
              />
              <p className="text-[10px] text-muted-foreground">
                ApexKit will download, extract, smoke-test, and apply the binary in the background.
              </p>
            </div>
          )}

          {/* Optional Checksum Field */}
          <div className="space-y-1.5">
            <Label>Expected SHA-256 Checksum (Optional)</Label>
            <Input
              placeholder="e.g. e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
              value={sha256Input}
              onChange={(e: any) => setSha256Input(e.target.value)}
              disabled={isSubmitting}
              className="font-mono text-xs"
            />
            <p className="text-[10px] text-muted-foreground">
              If provided, the hash is validated against the archive and the extracted binary before
              any swap occurs.
            </p>
          </div>

          <div className="pt-2 flex justify-end">
            <Button
              onClick={() => setConfirmUpgradeOpen(true)}
              isLoading={isSubmitting}
              disabled={
                isSubmitting ||
                (upgradeMode === 'upload' && !selectedFile) ||
                (upgradeMode === 'remote' && !remoteUrl.trim())
              }
              className="shadow-sm"
            >
              <HardDriveDownload className="mr-2 h-4 w-4" /> Review & Apply Upgrade
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* 3. Historical Rollbacks Table */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between pb-2">
          <CardTitle className="text-base flex items-center gap-2">
            <History className="h-4 w-4 text-muted-foreground" /> Archived Rollback Binaries (
            {rollbacks.length})
          </CardTitle>
          <Button variant="ghost" size="sm" onClick={loadRollbacks} disabled={isLoadingRollbacks}>
            <RefreshCw className={`h-4 w-4 ${isLoadingRollbacks ? 'animate-spin' : ''}`} />
          </Button>
        </CardHeader>
        <CardContent>
          <div className="rounded-md border border-border overflow-x-auto">
            <table className="w-full text-sm text-left">
              <thead className="bg-secondary/30 text-xs font-semibold text-muted-foreground">
                <tr>
                  <th className="px-4 py-3">Rollback Binary</th>
                  <th className="px-4 py-3 w-32 text-right">Size</th>
                  <th className="px-4 py-3 w-48 text-right">Archived At</th>
                  <th className="px-4 py-3 w-36 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border bg-card">
                {rollbacks.length === 0 ? (
                  <tr>
                    <td
                      colSpan={4}
                      className="p-8 text-center text-muted-foreground text-sm italic"
                    >
                      No archived rollback binaries found. A backup snapshot of the host executable
                      is created automatically before each upgrade.
                    </td>
                  </tr>
                ) : (
                  rollbacks.map((item) => (
                    <tr key={item.name} className="hover:bg-secondary/10 group transition-colors">
                      <td className="px-4 py-3 font-mono font-medium text-foreground">
                        <div className="flex items-center gap-2">
                          <RotateCcw className="h-3.5 w-3.5 text-primary opacity-80" />
                          <span>{item.name}</span>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-right font-mono text-xs text-muted-foreground whitespace-nowrap">
                        {formatFileSize(item.size)}
                      </td>
                      <td className="px-4 py-3 text-right font-mono text-xs text-muted-foreground whitespace-nowrap">
                        {item.created_at ? new Date(item.created_at).toLocaleString() : '-'}
                      </td>
                      <td className="px-4 py-3 text-right">
                        <div className="flex items-center justify-end gap-1.5">
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-7 text-xs px-2 text-amber-500 hover:text-amber-400 hover:bg-amber-500/10 border-amber-500/20"
                            onClick={() => setRollbackToTarget(item)}
                            title="Downgrade to this binary"
                          >
                            <RotateCcw className="h-3 w-3 mr-1" /> Rollback
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            className="h-7 w-7 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                            onClick={() => setRollbackToDelete(item)}
                            title="Delete archived binary"
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>

      {/* Confirmation: Upgrade Modal */}
      <ConfirmDialog
        isOpen={confirmUpgradeOpen}
        title="Confirm Host Binary Upgrade"
        description="The running binary will be smoke-tested, safely backed up into the rollbacks catalog, and replaced. A new process instance will spawn immediately to take over traffic without dropping requests. Proceed?"
        confirmText="Start Upgrade"
        variant="primary"
        isLoading={isSubmitting}
        onConfirm={handlePerformUpgrade}
        onCancel={() => setConfirmUpgradeOpen(false)}
      />

      {/* Confirmation: Rollback Modal */}
      <ConfirmDialog
        isOpen={!!rollbackToTarget}
        title={`Rollback to ${rollbackToTarget?.name}?`}
        description={`This will restore the executable '${rollbackToTarget?.name}' as the active host binary. The current running binary will be backed up, and the server process will initiate a zero-downtime handover to the previous version.`}
        confirmText="Confirm Rollback"
        variant="primary"
        isLoading={isSubmitting}
        onConfirm={handlePerformRollback}
        onCancel={() => setRollbackToTarget(null)}
      />

      {/* Confirmation: Delete Rollback Modal */}
      <ConfirmDialog
        isOpen={!!rollbackToDelete}
        title={`Delete Rollback Archive?`}
        description={`Are you sure you want to permanently delete '${rollbackToDelete?.name}' from disk? This cannot be undone.`}
        confirmText="Delete File"
        variant="destructive"
        onConfirm={handleDeleteRollback}
        onCancel={() => setRollbackToDelete(null)}
      />

      {/* Handover Polling Dialog */}
      <Dialog
        isOpen={isHandoverActive}
        onClose={() => {}}
        title="Server Handover in Progress"
        size="sm"
      >
        <div className="space-y-4 py-3 text-center">
          <div className="relative mx-auto w-12 h-12 flex items-center justify-center">
            <RefreshCw className="h-8 w-8 text-primary animate-spin" />
          </div>
          <div className="space-y-1">
            <h4 className="font-bold text-sm text-foreground">
              Draining Connections & Handing Over
            </h4>
            <p className="text-xs text-muted-foreground leading-relaxed">{handoverMessage}</p>
          </div>
          <div className="p-3 bg-secondary/20 rounded-lg border border-border text-[11px] font-mono text-muted-foreground">
            Healthcheck polling attempt #{pollAttempt}...
          </div>
        </div>
      </Dialog>
    </div>
  );
};
