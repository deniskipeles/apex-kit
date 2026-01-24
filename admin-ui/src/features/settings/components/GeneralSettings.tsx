import React, { useState, useEffect } from 'react';
import { Monitor, Moon, Sun, Upload, X, Image as ImageIcon, Save } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Input, Label, Button } from '../../../components/ui/Elements';
import { AppSettings } from '../../../types';
import { useTheme } from '../../../context/ThemeContext';
import { filesService } from '../../files/services/filesService';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '@/src/lib/apiClient';

interface GeneralSettingsProps {
    settings: AppSettings;
    onChange: (settings: Partial<AppSettings>) => void;
    onSave: (data: Partial<AppSettings>) => Promise<void>;
}

export const GeneralSettings = ({ settings, onChange, onSave }: GeneralSettingsProps) => {
    const { setTheme } = useTheme();
    const { toast } = useToast();
    const [isUploading, setIsUploading] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [isRoot, setIsRoot] = useState(false);

    useEffect(() => {
        // Check if we are in Root scope
        const scope = apiClient.getScope();
        setIsRoot(scope.type === 'root');
    }, []);

    const handleThemeChange = (t: 'light' | 'dark' | 'system') => {
        onChange({ theme: t });
        setTheme(t);
    };

    const handleLogoUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;

        setIsUploading(true);
        try {
            const uploaded = await filesService.upload(file);
            onChange({ appLogo: uploaded.name }); // Store filename
            toast('Logo uploaded successfully', 'success');
        } catch (err) {
            console.error(err);
            toast('Failed to upload logo', 'error');
        } finally {
            setIsUploading(false);
        }
    };

    const handleRemoveLogo = () => {
        onChange({ appLogo: '' });
    };

    const handleSaveClick = async () => {
        setIsSaving(true);
        try {
            await onSave({
                appName: settings.appName,
                appUrl: settings.appUrl,
                theme: settings.theme,
                appLogo: settings.appLogo,
                logoWidth: settings.logoWidth,
                logoHeight: settings.logoHeight,
                logRetentionDays: settings.logRetentionDays
            });
        } finally {
            setIsSaving(false);
        }
    };

    const logoUrl = settings.appLogo ? apiClient.files.getFileUrl(settings.appLogo) : null;

    return (
        <Card>
            <CardHeader>
                <CardTitle>Application Settings</CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">

                {/* BRANDING SECTION */}
                <div className="space-y-4 border-b border-border/50 pb-6">
                    <div className="flex items-center justify-between">
                        <Label className="text-base">App Logo & Branding</Label>
                    </div>

                    <div className="flex flex-col sm:flex-row gap-6 items-start">
                        {/* Logo Preview / Upload Area */}
                        <div className="flex flex-col gap-2">
                            <div className="h-32 w-32 rounded-lg border-2 border-dashed border-border flex items-center justify-center bg-secondary/10 overflow-hidden relative group">
                                {logoUrl ? (
                                    <>
                                        <img src={logoUrl} alt="App Logo" className="w-full h-full object-contain p-2" />
                                        <button onClick={handleRemoveLogo} className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center text-white"><X className="h-6 w-6" /></button>
                                    </>
                                ) : (
                                    <ImageIcon className="h-10 w-10 text-muted-foreground/50" />
                                )}

                                {isUploading && <div className="absolute inset-0 bg-background/80 flex items-center justify-center"><div className="animate-spin h-5 w-5 border-2 border-primary border-t-transparent rounded-full" /></div>}
                            </div>

                            <div className="flex gap-2">
                                <Button size="sm" variant="outline" className="w-32 relative" disabled={isUploading}>
                                    <Upload className="mr-2 h-3 w-3" /> {logoUrl ? 'Replace' : 'Upload'}
                                    <input type="file" accept="image/*" className="absolute inset-0 opacity-0 cursor-pointer" onChange={handleLogoUpload} />
                                </Button>
                            </div>
                        </div>

                        {/* Dimensions Inputs */}
                        <div className="flex-1 space-y-4 w-full">
                            <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                    <Label>Logo Width</Label>
                                    <Input placeholder="e.g. 150px, 4rem, auto" value={settings.logoWidth || ''} onChange={(e: any) => onChange({ logoWidth: e.target.value })} />
                                </div>
                                <div className="space-y-2">
                                    <Label>Logo Height</Label>
                                    <Input placeholder="e.g. 40px, auto" value={settings.logoHeight || ''} onChange={(e: any) => onChange({ logoHeight: e.target.value })} />
                                </div>
                            </div>
                            <div className="space-y-2">
                                <Label>Application Name</Label>
                                <Input value={settings.appName} onChange={(e: any) => onChange({ appName: e.target.value })} />
                            </div>
                        </div>
                    </div>
                </div>

                {/* GENERAL CONFIG */}
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                    <div className="space-y-2">
                        <Label>Application URL</Label>
                        <Input value={settings.appUrl} onChange={(e: any) => onChange({ appUrl: e.target.value })} />
                    </div>
                    <div className="space-y-2">
                        <Label>Log Retention (Days)</Label>
                        <Input type="number" value={settings.logRetentionDays ? Number(settings.logRetentionDays) : 7} onChange={(e: any) => onChange({ logRetentionDays: Number(e.target.value) })} />
                    </div>
                    {/* Max Site Size (Root Only) */}
                    {isRoot && (
                        <div className="space-y-2">
                            <Label>Max Static Site Size (MB)</Label>
                            <Input
                                type="number"
                                value={settings.maxSiteSizeMb || 50}
                                onChange={(e: any) => onChange({ maxSiteSizeMb: Number(e.target.value) })}
                            />
                            <p className="text-[10px] text-muted-foreground">Global limit for 'Hosting' deployments. Enforced for all tenants.</p>
                        </div>
                    )}
                </div>

                <div className="space-y-2">
                    <Label>Appearance</Label>
                    <div className="grid grid-cols-3 gap-2 sm:gap-4">
                        {['light', 'dark', 'system'].map((theme) => (
                            <Button
                                key={theme}
                                variant={settings.theme === theme ? 'primary' : 'outline'}
                                className="h-16 sm:h-20 flex flex-col gap-2"
                                onClick={() => handleThemeChange(theme as any)}
                            >
                                {theme === 'light' ? <Sun className="h-5 w-5" /> : theme === 'dark' ? <Moon className="h-5 w-5" /> : <Monitor className="h-5 w-5" />}
                                <span className="text-xs sm:text-sm capitalize">{theme}</span>
                            </Button>
                        ))}
                    </div>
                </div>

                <div className="border-t border-border pt-6 flex justify-end">
                    <Button onClick={handleSaveClick} isLoading={isSaving}>
                        <Save className="mr-2 h-4 w-4" /> Save General Settings
                    </Button>
                </div>
            </CardContent>
        </Card>
    );
};