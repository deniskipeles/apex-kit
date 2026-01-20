import React, { useState, useEffect } from 'react';
import { Globe, Upload, RefreshCw, FileText, ExternalLink, CheckCircle } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Button, Label, Input } from '../../../components/ui/Elements';
import { apiClient } from '../../../lib/apiClient';
import { SiteFile } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { formatFileSize } from '../../../lib/formatters';

export const SiteSettings = () => {
    const [files, setFiles] = useState<SiteFile[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [isUploading, setIsUploading] = useState(false);
    const { toast } = useToast();

    // Determine current public URL based on browser location
    // If in /_dashboard/tenant/xyz, public url is /tenant/xyz
    // If in /_dashboard, public url is /
    const getPublicUrl = () => {
        const path = window.location.pathname;
        if (path.includes('/tenant/')) {
            const tenantId = path.split('/tenant/')[1].split('/')[0];
            return `${window.location.origin}/tenant/${tenantId}`;
        }
        if (path.includes('/sandbox/')) {
            const sandboxId = path.split('/sandbox/')[1].split('/')[0];
            return `${window.location.origin}/sandbox/${sandboxId}`;
        }
        return window.location.origin;
    };

    const publicUrl = getPublicUrl();

    const loadFiles = async () => {
        setIsLoading(true);
        try {
            const list = await apiClient.sites.list();
            // Sort by path
            list.sort((a, b) => a.path.localeCompare(b.path));
            setFiles(list);
        } catch (e) {
            toast('Failed to load site files', 'error');
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => {
        loadFiles();
    }, []);

    const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;

        if (!file.name.endsWith('.zip')) {
            toast('Please upload a .zip file', 'error');
            return;
        }

        setIsUploading(true);
        try {
            await apiClient.sites.deploy(file);
            toast('Site deployed successfully', 'success');
            loadFiles();
        } catch (e: any) {
            toast(e.message || 'Deployment failed', 'error');
        } finally {
            setIsUploading(false);
            // Reset input
            e.target.value = '';
        }
    };

    return (
        <div className="space-y-6">
            <Card>
                <CardHeader className="flex flex-row items-center justify-between">
                    <CardTitle className="flex items-center gap-2">
                        <Globe className="h-4 w-4" /> Static Hosting
                    </CardTitle>
                    <a href={publicUrl} target="_blank" rel="noreferrer">
                        <Button variant="outline" size="sm" className="gap-2">
                            <ExternalLink className="h-3.5 w-3.5" /> Visit Site
                        </Button>
                    </a>
                </CardHeader>
                <CardContent className="space-y-6">
                    
                    {/* Upload Section */}
                    <div className="rounded-lg border border-dashed border-border bg-secondary/5 p-8 flex flex-col items-center justify-center text-center">
                        <div className="h-12 w-12 rounded-full bg-primary/10 flex items-center justify-center mb-4 text-primary">
                            <Upload className="h-6 w-6" />
                        </div>
                        <h3 className="text-lg font-semibold">Deploy New Version</h3>
                        <p className="text-sm text-muted-foreground max-w-sm mt-1 mb-4">
                            Upload a <code>.zip</code> file containing your HTML, CSS, and JS. The contents will be extracted to the public root.
                        </p>
                        
                        <div className="relative">
                            <Button disabled={isUploading}>
                                {isUploading ? 'Deploying...' : 'Select ZIP File'}
                            </Button>
                            <input 
                                type="file" 
                                accept=".zip" 
                                className="absolute inset-0 opacity-0 cursor-pointer" 
                                onChange={handleUpload}
                                disabled={isUploading}
                            />
                        </div>
                        {isUploading && <p className="text-xs text-muted-foreground mt-2 animate-pulse">Extracting files...</p>}
                    </div>

                    {/* File List */}
                    <div className="space-y-3">
                        <div className="flex items-center justify-between">
                            <Label>Deployed Files ({files.length})</Label>
                            <Button variant="ghost" size="sm" onClick={loadFiles} disabled={isLoading}>
                                <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? 'animate-spin' : ''}`} />
                            </Button>
                        </div>

                        <div className="rounded-md border border-border bg-card overflow-hidden max-h-[400px] overflow-y-auto">
                            {files.length === 0 ? (
                                <div className="p-8 text-center text-muted-foreground text-sm">
                                    No custom files deployed. Serving default system UI.
                                </div>
                            ) : (
                                <div className="divide-y divide-border">
                                    {files.map((file, idx) => (
                                        <div key={idx} className="flex items-center justify-between p-3 hover:bg-secondary/20 text-sm">
                                            <div className="flex items-center gap-3 overflow-hidden">
                                                <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
                                                <span className="font-mono truncate text-foreground/90">{file.path}</span>
                                                {file.path === 'index.html' && (
                                                    <span className="text-[10px] bg-emerald-500/10 text-emerald-500 px-1.5 rounded flex items-center gap-1">
                                                        <CheckCircle className="h-3 w-3" /> Entry
                                                    </span>
                                                )}
                                            </div>
                                            <span className="text-xs text-muted-foreground font-mono shrink-0">
                                                {formatFileSize(file.size)}
                                            </span>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>

                </CardContent>
            </Card>
        </div>
    );
};