import React, { useState, useEffect } from 'react';
import {
  Sparkles,
  Plus,
  Trash2,
  Copy,
  Check,
  RefreshCw,
  ExternalLink,
  Code,
  Image as ImageIcon,
  Type,
} from 'lucide-react';
import { Dialog } from '../../../components/ui/Dialog';
import { Button, Input, Select, Label, Badge } from '../../../components/ui/Elements';
import { useToast } from '../../../components/feedback/Toast';
import { templatesService } from '../../templates/services/templatesService';
import { Template } from '../../../types';
import { APP_CONFIG } from '../../../config/app.config';

interface OpenGraphTesterModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface OgDataItem {
  type: 'text' | 'image';
  target: string;
  value: string;
}

export const OpenGraphTesterModal = ({ isOpen, onClose }: OpenGraphTesterModalProps) => {
  const { toast } = useToast();
  const [templates, setTemplates] = useState<Template[]>([]);
  const [templateSlug, setTemplateSlug] = useState('');
  const [format, setFormat] = useState<'png' | 'webp' | 'jpeg'>('png');
  const [quality, setQuality] = useState(80);

  // Initial preset variables
  const [dataItems, setDataItems] = useState<OgDataItem[]>([
    { type: 'text', target: 'TITLE_LINE_1', value: 'Dynamic OpenGraph Studio' },
    { type: 'text', target: 'TITLE_LINE_2', value: 'Built into ApexKit BaaS' },
    { type: 'text', target: 'SITE_NAME', value: 'apexkit.io' },
  ]);

  const [previewUrl, setPreviewUrl] = useState('');
  const [isLoadingImage, setIsLoadingImage] = useState(false);
  const [copiedUrl, setCopiedUrl] = useState(false);
  const [copiedMeta, setCopiedMeta] = useState(false);

  // 1. Fetch available templates on open
  useEffect(() => {
    if (isOpen) {
      templatesService.list().then((tmpls) => {
        setTemplates(tmpls);
        if (tmpls.length > 0 && !templateSlug) {
          setTemplateSlug(tmpls[0].slug);
        }
      });
    }
  }, [isOpen]);

  // 2. Build live URL string
  const buildUrl = () => {
    if (!templateSlug) return '';
    const params = new URLSearchParams();
    params.append('template', templateSlug);
    params.append('data', JSON.stringify(dataItems));
    if (format !== 'png') params.append('format', format);
    if (quality !== 80) params.append('quality', String(quality));

    return `${APP_CONFIG.apiBaseUrl}/api/v1/storage/files/opengraph?${params.toString()}`;
  };

  const handleGenerate = () => {
    setIsLoadingImage(true);
    const url = buildUrl();
    setPreviewUrl(url);
  };

  useEffect(() => {
    if (templateSlug) {
      handleGenerate();
    }
  }, [templateSlug, format, quality]);

  // Item Management Handlers
  const handleAddItem = () => {
    if (dataItems.length >= 8) {
      toast('Maximum 8 variables allowed', 'warning');
      return;
    }
    setDataItems([...dataItems, { type: 'text', target: 'NEW_VAR', value: 'Sample Text' }]);
  };

  const handleRemoveItem = (index: number) => {
    setDataItems(dataItems.filter((_, i) => i !== index));
  };

  const handleUpdateItem = (index: number, field: keyof OgDataItem, value: string) => {
    const updated = [...dataItems];
    updated[index] = { ...updated[index], [field]: value };
    setDataItems(updated);
  };

  const currentUrl = previewUrl || buildUrl();
  const metaTag = `<meta property="og:image" content="${currentUrl}" />`;

  const copyToClipboard = (text: string, isMeta: boolean) => {
    navigator.clipboard.writeText(text);
    if (isMeta) {
      setCopiedMeta(true);
      setTimeout(() => setCopiedMeta(false), 2000);
    } else {
      setCopiedUrl(true);
      setTimeout(() => setCopiedUrl(false), 2000);
    }
    toast('Copied to clipboard!', 'success');
  };

  if (!isOpen) return null;

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title="OpenGraph Studio & Preview" size="xl">
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-6 min-h-[600px] pb-4">
        {/* Left Column: Form Controls */}
        <div className="lg:col-span-5 space-y-5 flex flex-col h-full border-r border-border pr-0 lg:pr-4">
          {/* Template & Output Format */}
          <div className="space-y-4 p-4 rounded-xl bg-secondary/10 border border-border">
            <div className="space-y-1.5">
              <Label required>SVG Template (Slug)</Label>
              {templates.length > 0 ? (
                <Select value={templateSlug} onChange={(e: any) => setTemplateSlug(e.target.value)}>
                  {templates.map((t) => (
                    <option key={t.id} value={t.slug}>
                      {t.slug}
                    </option>
                  ))}
                </Select>
              ) : (
                <Input
                  value={templateSlug}
                  onChange={(e: any) => setTemplateSlug(e.target.value)}
                  placeholder="e.g. og-template-1"
                />
              )}
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label>Format</Label>
                <Select value={format} onChange={(e: any) => setFormat(e.target.value)}>
                  <option value="png">PNG</option>
                  <option value="webp">WebP</option>
                  <option value="jpeg">JPEG</option>
                </Select>
              </div>

              <div className="space-y-1.5">
                <Label>Quality ({quality}%)</Label>
                <input
                  type="range"
                  min="10"
                  max="100"
                  value={quality}
                  onChange={(e) => setQuality(Number(e.target.value))}
                  className="w-full h-9 cursor-pointer accent-primary"
                />
              </div>
            </div>
          </div>

          {/* Dynamic Variables Array (Max 8) */}
          <div className="flex-1 flex flex-col space-y-3 min-h-[250px]">
            <div className="flex items-center justify-between">
              <Label className="flex items-center gap-2">
                Template Data Injection
                <Badge variant="secondary" className="font-mono text-[10px]">
                  {dataItems.length}/8
                </Badge>
              </Label>
              <Button
                size="sm"
                variant="outline"
                className="h-7 text-xs"
                onClick={handleAddItem}
                disabled={dataItems.length >= 8}
              >
                <Plus className="h-3 w-3 mr-1" /> Add Var
              </Button>
            </div>

            <div className="flex-1 overflow-y-auto space-y-3.5 pr-1 custom-scrollbar max-h-[300px]">
              {dataItems.map((item, idx) => (
                <div
                  key={idx}
                  className="p-2.5 rounded-lg border border-border bg-card space-y-2 relative group"
                >
                  <div className="flex items-center justify-between gap-2">
                    <Select
                      className="h-7 text-xs w-28"
                      value={item.type}
                      onChange={(e: any) => handleUpdateItem(idx, 'type', e.target.value)}
                    >
                      <option value="text">Text</option>
                      <option value="image">Image / Logo</option>
                    </Select>

                    <Input
                      placeholder="Tera Var (e.g. TITLE)"
                      value={item.target}
                      onChange={(e: any) => handleUpdateItem(idx, 'target', e.target.value)}
                      className="h-7 font-mono text-xs flex-1"
                    />

                    <Button
                      size="icon"
                      variant="ghost"
                      className="h-7 w-7 text-muted-foreground hover:text-destructive shrink-0"
                      onClick={() => handleRemoveItem(idx)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>

                  <Input
                    placeholder={
                      item.type === 'text' ? 'Text value...' : 'filename.png or data:image/...'
                    }
                    value={item.value}
                    onChange={(e: any) => handleUpdateItem(idx, 'value', e.target.value)}
                    className="h-8 text-xs font-mono"
                  />
                </div>
              ))}
            </div>
          </div>

          <Button className="w-full h-10 font-bold shadow-md" onClick={handleGenerate}>
            <Sparkles className="mr-2 h-4 w-4" /> Update Preview
          </Button>
        </div>

        {/* Right Column: Live Card Preview & Code snippet */}
        <div className="lg:col-span-7 flex flex-col h-full space-y-4">
          <div className="flex justify-between items-center">
            <span className="text-xs font-bold text-muted-foreground uppercase tracking-wider">
              1200x630 Social Card Preview
            </span>
            <Badge variant="outline" className="font-mono text-[10px] uppercase">
              {format} • {quality}%
            </Badge>
          </div>

          {/* Canvas Wrapper */}
          <div className="relative w-full aspect-[1200/630] rounded-xl border border-border bg-[#0d1117] overflow-hidden flex items-center justify-center shadow-2xl group">
            {isLoadingImage && (
              <div className="absolute inset-0 bg-background/60 backdrop-blur-sm z-10 flex flex-col items-center justify-center gap-2">
                <RefreshCw className="h-8 w-8 animate-spin text-primary" />
                <span className="text-xs font-mono text-muted-foreground">Rendering Canvas...</span>
              </div>
            )}

            {currentUrl ? (
              <img
                src={currentUrl}
                alt="OpenGraph Preview"
                className="w-full h-full object-contain"
                onLoad={() => setIsLoadingImage(false)}
                onError={() => {
                  setIsLoadingImage(false);
                  toast('Failed to render SVG image preview', 'error');
                }}
              />
            ) : (
              <div className="text-center text-muted-foreground/40 text-sm italic">
                Select a template to render preview
              </div>
            )}

            {currentUrl && (
              <a
                href={currentUrl}
                target="_blank"
                rel="noreferrer"
                className="absolute top-3 right-3 p-2 bg-black/70 hover:bg-black text-white rounded-lg opacity-0 group-hover:opacity-100 transition-opacity backdrop-blur"
                title="Open Direct Image"
              >
                <ExternalLink className="h-4 w-4" />
              </a>
            )}
          </div>

          {/* Output Code / Meta Tag Section */}
          <div className="space-y-3 pt-2">
            <div className="space-y-1">
              <div className="flex justify-between items-center">
                <Label className="text-xs flex items-center gap-1.5">
                  <Code className="h-3.5 w-3.5 text-primary" /> Generated HTML Meta Tag
                </Label>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-6 text-[10px]"
                  onClick={() => copyToClipboard(metaTag, true)}
                >
                  {copiedMeta ? (
                    <Check className="h-3 w-3 text-emerald-500 mr-1" />
                  ) : (
                    <Copy className="h-3 w-3 mr-1" />
                  )}
                  Copy Tag
                </Button>
              </div>
              <pre className="p-2.5 rounded-lg bg-[#161b22] border border-border font-mono text-[11px] text-emerald-400 overflow-x-auto whitespace-pre-wrap break-all select-all">
                {metaTag}
              </pre>
            </div>

            <div className="space-y-1">
              <div className="flex justify-between items-center">
                <Label className="text-xs">Direct Endpoint GET URL</Label>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-6 text-[10px]"
                  onClick={() => copyToClipboard(currentUrl, false)}
                >
                  {copiedUrl ? (
                    <Check className="h-3 w-3 text-emerald-500 mr-1" />
                  ) : (
                    <Copy className="h-3 w-3 mr-1" />
                  )}
                  Copy Link
                </Button>
              </div>
              <input
                readOnly
                value={currentUrl}
                className="w-full h-8 px-2.5 rounded-lg bg-secondary/30 border border-border font-mono text-[11px] text-muted-foreground select-all"
              />
            </div>
          </div>
        </div>
      </div>
    </Dialog>
  );
};
