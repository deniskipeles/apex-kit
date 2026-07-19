import React, { useState, useEffect } from 'react';
import { Plus, Trash2, Edit, LayoutTemplate, ExternalLink } from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { TemplateEditor } from '../components/TemplateEditor';
import { templatesService } from '../services/templatesService';
import { Template } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { APP_CONFIG } from '../../../config/app.config';
import { ImportExportToolbar } from '@/src/components/ImportExportToolbar';

export const TemplatesPage = () => {
  const [templates, setTemplates] = useState<Template[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [editorOpen, setEditorOpen] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState<Template | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const { toast } = useToast();

  const loadTemplates = async () => {
    setIsLoading(true);
    try {
      const data = await templatesService.list();
      setTemplates(data);
    } catch (e) {
      toast('Failed to load templates', 'error');
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadTemplates();
  }, []);

  const handleSave = async (data: Partial<Template>) => {
    if (selectedTemplate) {
      await templatesService.update(selectedTemplate.id, data);
      toast('Template updated', 'success');
    } else {
      await templatesService.create(data);
      toast('Template created', 'success');
    }
    loadTemplates();
  };

  const handleDelete = async () => {
    if (!deleteId) return;
    await templatesService.delete(deleteId);
    toast('Template deleted', 'success');
    setDeleteId(null);
    loadTemplates();
  };

  // --- DYNAMIC RENDERING PATH RESOLVER ---
  const getRenderUrl = (slug: string) => {
    const path = window.location.pathname;
    const tenantMatch = path.match(/^\/_dashboard\/tenant\/([^/]+)/);
    if (tenantMatch) {
      return `${APP_CONFIG.apiBaseUrl}/tenant/${tenantMatch[1]}/render/${slug}`;
    }
    const sandboxMatch = path.match(/^\/_dashboard\/sandbox\/([^/]+)/);
    if (sandboxMatch) {
      return `${APP_CONFIG.apiBaseUrl}/sandbox/${sandboxMatch[1]}/render/${slug}`;
    }
    return `${APP_CONFIG.apiBaseUrl}/render/${slug}`;
  };

  const columns = [
    {
      field: 'slug',
      headerName: 'Slug',
      renderCell: (t: Template) => (
        <div className="flex flex-col">
          <span className="font-medium font-mono text-primary">{t.slug}</span>
        </div>
      ),
    },
    {
      field: 'script_id',
      headerName: 'Linked Script',
      width: '150px',
      renderCell: (t: Template) =>
        t.script_id ? (
          <Badge variant="secondary">Script #{t.script_id}</Badge>
        ) : (
          <span className="text-muted-foreground text-xs">-</span>
        ),
    },
    {
      field: 'actions',
      headerName: '',
      align: 'right' as const,
      width: '150px',
      renderCell: (t: Template) => (
        <div className="flex justify-end gap-1">
          <a href={getRenderUrl(t.slug)} target="_blank" rel="noreferrer">
            <Button size="icon" variant="ghost" title="View Rendered">
              <ExternalLink className="h-4 w-4 text-muted-foreground" />
            </Button>
          </a>
          <Button
            size="icon"
            variant="ghost"
            onClick={(e: any) => {
              e.stopPropagation();
              setSelectedTemplate(t);
              setEditorOpen(true);
            }}
          >
            <Edit className="h-4 w-4 text-muted-foreground hover:text-primary" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            onClick={(e: any) => {
              e.stopPropagation();
              setDeleteId(t.id);
            }}
          >
            <Trash2 className="h-4 w-4 text-muted-foreground hover:text-destructive" />
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Templates</h2>
          <p className="text-muted-foreground">HTML/HTMX templates for dynamic rendering.</p>
        </div>
        <div className="flex gap-2">
          <ImportExportToolbar
            onExport={(format) => templatesService.export(format)}
            onImport={templatesService.import}
          />
          <Button
            onClick={() => {
              setSelectedTemplate(null);
              setEditorOpen(true);
            }}
          >
            <Plus className="mr-2 h-4 w-4" /> New Template
          </Button>
        </div>
      </div>

      <DataGrid data={templates} columns={columns} keyField="id" isLoading={isLoading} />

      <TemplateEditor
        isOpen={editorOpen}
        onClose={() => setEditorOpen(false)}
        onSave={handleSave}
        initialData={selectedTemplate || undefined}
      />

      <ConfirmDialog
        isOpen={!!deleteId}
        title="Delete Template"
        description="Are you sure? This page will stop rendering."
        onConfirm={handleDelete}
        onCancel={() => setDeleteId(null)}
      />
    </div>
  );
};
