import React, { useState } from 'react';
import { HardDrive, Cloud, Server, ArrowRightLeft, Save } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Select, Switch, Button } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { apiClient } from '@/src/lib/apiClient';
import { useToast } from '@/src/components/feedback/Toast';

interface StorageSettingsProps {
    settings: AppSettings;
    onChange: (settings: Partial<AppSettings>) => void;
    onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const StorageSettings = ({ settings, onChange, onSave }: StorageSettingsProps) => {
    const { toast } = useToast();
    const [isTesting, setIsTesting] = useState(false);
    const [isMigrating, setIsMigrating] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [showMigrationModal, setShowMigrationModal] = useState(false);
    const [migrationDirection, setMigrationDirection] = useState<'local_to_s3' | 's3_to_local'>('local_to_s3');

    const updateStorage = (updates: any) => { onChange({ storage: { ...settings.storage, ...updates } }); };
    const updateS3 = (updates: any) => { onChange({ storage: { ...settings.storage, s3: { ...settings.storage.s3, ...updates } } }); };

    const handleTestConnection = async () => {
        setIsTesting(true);
        try {
            await apiClient.testS3Connection(settings.storage.s3);
            toast("Connection successful! Read/Write verified.", "success");
        } catch (e: any) {
            console.error(e);
            toast(`Connection failed: ${e.message}`, "error");
        } finally {
            setIsTesting(false);
        }
    };

    const handleMigration = async () => {
        setIsMigrating(true);
        setShowMigrationModal(false);
        const source = migrationDirection === 'local_to_s3' ? 'local' : 's3';
        const destination = migrationDirection === 'local_to_s3' ? 's3' : 'local';
        try {
            toast(`Starting migration: ${source} -> ${destination}...`, 'info');
            const res = await apiClient.migrateStorage(source, destination);
            if (res.errors > 0) { toast(`Migration completed with issues. Processed: ${res.processed}, Errors: ${res.errors}`, 'warning'); } 
            else { toast(`Success! Migrated ${res.processed} files.`, 'success'); }
        } catch (e: any) {
            console.error(e);
            toast(`Migration failed: ${e.message}`, 'error');
        } finally {
            setIsMigrating(false);
        }
    };
    
    const handleSaveClick = async () => {
        setIsSaving(true);
        try {
            await onSave({ storage: settings.storage });
        } finally {
            setIsSaving(false);
        }
    };

    return (
        <div className="space-y-6">
            <Card>
                <CardHeader><CardTitle className="flex items-center gap-2"><HardDrive className="h-4 w-4" /> Active Storage Driver</CardTitle></CardHeader>
                <CardContent className="space-y-6">
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <button className={`flex items-center gap-4 p-4 rounded-lg border transition-all text-left ${settings.storage.activeDriver === 'local' ? 'border-primary bg-primary/5 ring-1 ring-primary' : 'border-border hover:bg-secondary/50'}`} onClick={() => updateStorage({ activeDriver: 'local' })}>
                            <div className="p-2 rounded-full bg-secondary"><Server className="h-6 w-6 text-foreground" /></div>
                            <div><div className="font-semibold">Local Storage</div><div className="text-xs text-muted-foreground">Store files on the server disk.</div></div>
                        </button>
                        <button className={`flex items-center gap-4 p-4 rounded-lg border transition-all text-left ${settings.storage.activeDriver === 's3' ? 'border-primary bg-primary/5 ring-1 ring-primary' : 'border-border hover:bg-secondary/50'}`} onClick={() => updateStorage({ activeDriver: 's3' })}>
                            <div className="p-2 rounded-full bg-secondary"><Cloud className="h-6 w-6 text-foreground" /></div>
                            <div><div className="font-semibold">S3 Object Storage</div><div className="text-xs text-muted-foreground">AWS S3, Google Cloud, MinIO, etc.</div></div>
                        </button>
                    </div>
                </CardContent>
            </Card>

            <Card className={`transition-opacity duration-300 ${settings.storage.activeDriver === 's3' ? 'opacity-100' : 'opacity-50 pointer-events-none grayscale'}`}>
                <CardHeader>
                    <div className="flex items-center justify-between">
                        <CardTitle>S3 Configuration</CardTitle>
                        <Switch checked={settings.storage.s3.enabled} onCheckedChange={(c: boolean) => updateS3({ enabled: c })} />
                    </div>
                </CardHeader>
                <CardContent className="space-y-4">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div className="space-y-2"><Label>Provider</Label><Select value={settings.storage.s3.provider} onChange={(e: any) => updateS3({ provider: e.target.value })}><option value="aws">AWS S3</option><option value="gcs">Google Cloud Storage</option><option value="digitalocean">DigitalOcean Spaces</option><option value="minio">MinIO</option><option value="other">Other S3 Compatible</option></Select></div>
                        <div className="space-y-2"><Label>Region</Label><Input value={settings.storage.s3.region} onChange={(e: any) => updateS3({ region: e.target.value })} placeholder="us-east-1" /></div>
                    </div>
                    <div className="space-y-2"><Label>Bucket Name</Label><Input value={settings.storage.s3.bucket} onChange={(e: any) => updateS3({ bucket: e.target.value })} placeholder="my-app-uploads" /></div>
                    <div className="space-y-2"><Label>Endpoint URL</Label><Input value={settings.storage.s3.endpoint} onChange={(e: any) => updateS3({ endpoint: e.target.value })} placeholder="https://s3.amazonaws.com" /><p className="text-[10px] text-muted-foreground">Leave empty for standard AWS S3.</p></div>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div className="space-y-2"><Label>Access Key</Label><Input value={settings.storage.s3.accessKey} onChange={(e: any) => updateS3({ accessKey: e.target.value })} type="password" /></div>
                        <div className="space-y-2"><Label>Secret Key</Label><Input value={settings.storage.s3.secretKey} onChange={(e: any) => updateS3({ secretKey: e.target.value })} type="password" /></div>
                    </div>
                    <div className="flex justify-end pt-2">
                        <Button type="button" variant="outline" size="sm" onClick={handleTestConnection} isLoading={isTesting} disabled={!settings.storage.s3.bucket}>{isTesting ? 'Testing...' : 'Test Connection'}</Button>
                    </div>
                </CardContent>
            </Card>

            <Card className="border-dashed border-border bg-secondary/5">
                <CardHeader><CardTitle className="flex items-center gap-2 text-base"><ArrowRightLeft className="h-4 w-4" /> Data Migration</CardTitle></CardHeader>
                <CardContent>
                    <div className="flex flex-col md:flex-row items-center justify-between gap-4">
                        <div className="text-sm text-muted-foreground">Move existing files between Local Storage and S3.</div>
                        <div className="flex items-center gap-2">
                            <Select value={migrationDirection} onChange={(e: any) => setMigrationDirection(e.target.value)} className="w-40 h-9 text-xs"><option value="local_to_s3">Local -&gt; S3</option><option value="s3_to_local">S3 -&gt; Local</option></Select>
                            <Button variant="outline" size="sm" onClick={() => setShowMigrationModal(true)} isLoading={isMigrating}>Sync Files</Button>
                        </div>
                    </div>
                </CardContent>
            </Card>

            {showMigrationModal && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
                    <div className="bg-background border border-border rounded-lg shadow-xl p-6 max-w-sm w-full animate-in zoom-in-95">
                        <h3 className="text-lg font-bold mb-2">Confirm Migration</h3>
                        <p className="text-sm text-muted-foreground mb-4">This will copy all files between storages. Process runs in the background.</p>
                        <div className="flex justify-end gap-2"><Button variant="ghost" onClick={() => setShowMigrationModal(false)}>Cancel</Button><Button onClick={handleMigration}>Start Migration</Button></div>
                    </div>
                </div>
            )}
            
            <div className="flex justify-end">
                <Button onClick={handleSaveClick} isLoading={isSaving} className="w-full sm:w-auto">
                    <Save className="mr-2 h-4 w-4" /> Save Storage Settings
                </Button>
            </div>
        </div>
    );
};