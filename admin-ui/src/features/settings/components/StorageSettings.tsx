
import React from 'react';
import { HardDrive, Cloud, Server } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Select, Switch } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';

interface StorageSettingsProps {
  settings: AppSettings;
  onChange: (settings: Partial<AppSettings>) => void;
}

export const StorageSettings = ({ settings, onChange }: StorageSettingsProps) => {
  const updateStorage = (updates: any) => {
      onChange({ storage: { ...settings.storage, ...updates } });
  };
  
  const updateS3 = (updates: any) => {
      onChange({ 
          storage: { 
              ...settings.storage, 
              s3: { ...settings.storage.s3, ...updates } 
          } 
      });
  };

  return (
    <div className="space-y-6">
        <Card>
            <CardHeader>
                <CardTitle className="flex items-center gap-2"><HardDrive className="h-4 w-4" /> Active Storage Driver</CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <button 
                        className={`flex items-center gap-4 p-4 rounded-lg border transition-all text-left ${settings.storage.activeDriver === 'local' ? 'border-primary bg-primary/5 ring-1 ring-primary' : 'border-border hover:bg-secondary/50'}`}
                        onClick={() => updateStorage({ activeDriver: 'local' })}
                    >
                        <div className="p-2 rounded-full bg-secondary">
                            <Server className="h-6 w-6 text-foreground" />
                        </div>
                        <div>
                            <div className="font-semibold">Local Storage</div>
                            <div className="text-xs text-muted-foreground">Store files on the server disk.</div>
                        </div>
                    </button>

                    <button 
                        className={`flex items-center gap-4 p-4 rounded-lg border transition-all text-left ${settings.storage.activeDriver === 's3' ? 'border-primary bg-primary/5 ring-1 ring-primary' : 'border-border hover:bg-secondary/50'}`}
                        onClick={() => updateStorage({ activeDriver: 's3' })}
                    >
                        <div className="p-2 rounded-full bg-secondary">
                            <Cloud className="h-6 w-6 text-foreground" />
                        </div>
                        <div>
                            <div className="font-semibold">S3 Object Storage</div>
                            <div className="text-xs text-muted-foreground">AWS S3, Google Cloud, MinIO, etc.</div>
                        </div>
                    </button>
                </div>
            </CardContent>
        </Card>

        <Card className={`transition-opacity duration-300 ${settings.storage.activeDriver === 's3' ? 'opacity-100' : 'opacity-50 pointer-events-none grayscale'}`}>
            <CardHeader>
                <div className="flex items-center justify-between">
                    <CardTitle>S3 Configuration</CardTitle>
                    <Switch 
                        checked={settings.storage.s3.enabled}
                        onCheckedChange={(c: boolean) => updateS3({ enabled: c })}
                    />
                </div>
            </CardHeader>
            <CardContent className="space-y-4">
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div className="space-y-2">
                        <Label>Provider</Label>
                        <Select 
                            value={settings.storage.s3.provider}
                            onChange={(e: any) => updateS3({ provider: e.target.value })}
                        >
                            <option value="aws">AWS S3</option>
                            <option value="gcs">Google Cloud Storage</option>
                            <option value="digitalocean">DigitalOcean Spaces</option>
                            <option value="minio">MinIO (Self Hosted)</option>
                            <option value="other">Other S3 Compatible</option>
                        </Select>
                    </div>
                     <div className="space-y-2">
                        <Label>Region</Label>
                        <Input 
                            value={settings.storage.s3.region}
                            onChange={(e: any) => updateS3({ region: e.target.value })}
                            placeholder="us-east-1"
                        />
                    </div>
                </div>

                <div className="space-y-2">
                    <Label>Bucket Name</Label>
                    <Input 
                        value={settings.storage.s3.bucket}
                        onChange={(e: any) => updateS3({ bucket: e.target.value })}
                        placeholder="my-app-uploads"
                    />
                </div>

                <div className="space-y-2">
                    <Label>Endpoint URL</Label>
                    <Input 
                        value={settings.storage.s3.endpoint}
                        onChange={(e: any) => updateS3({ endpoint: e.target.value })}
                        placeholder="https://s3.amazonaws.com"
                    />
                    <p className="text-[10px] text-muted-foreground">Leave empty for standard AWS S3.</p>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div className="space-y-2">
                        <Label>Access Key</Label>
                        <Input 
                            value={settings.storage.s3.accessKey}
                            onChange={(e: any) => updateS3({ accessKey: e.target.value })}
                            type="password"
                        />
                    </div>
                    <div className="space-y-2">
                        <Label>Secret Key</Label>
                        <Input 
                            value={settings.storage.s3.secretKey}
                            onChange={(e: any) => updateS3({ secretKey: e.target.value })}
                            type="password"
                        />
                    </div>
                </div>
            </CardContent>
        </Card>
    </div>
  );
};
