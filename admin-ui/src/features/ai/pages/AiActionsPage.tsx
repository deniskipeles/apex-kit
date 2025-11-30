import React, { useState, useEffect } from 'react';
import { Plus, Trash2, Zap, Terminal } from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { AiActionEditor } from '../components/AiActionEditor';
import { AiActionTester } from '../components/AiActionTester';
import { aiService } from '../services/aiService';
import { AiAction } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';

export const AiActionsPage = () => {
  const [actions, setActions] = useState<AiAction[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [editorOpen, setEditorOpen] = useState(false);
  const [testerOpen, setTesterOpen] = useState(false);
  const [selectedAction, setSelectedAction] = useState<AiAction | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  
  const { toast } = useToast();

  const loadActions = async () => {
      setIsLoading(true);
      try {
          const data = await aiService.list();
          setActions(data);
      } catch (e) {
        console.log(e)
          toast('Failed to load actions', 'error');
      } finally {
          setIsLoading(false);
      }
  };

  useEffect(() => { loadActions(); }, []);

  const handleSave = async (data: Partial<AiAction>) => {
      // Note: Backend doesn't support UPDATE yet for AI Actions in previous steps, only Create/Delete.
      // If you added update, call update here. Otherwise, delete & re-create or show error.
      // Assuming Create Only for now based on backend code provided previously.
      try {
          await aiService.create(data);
          toast('Action created', 'success');
          loadActions();
      } catch (e) {
          toast('Failed to save', 'error');
      }
  };

  const handleDelete = async () => {
      if(!deleteId) return;
      await aiService.delete(deleteId);
      toast('Action deleted', 'success');
      setDeleteId(null);
      loadActions();
  };

  const columns = [
      { 
          field: 'name', headerName: 'Name', 
          renderCell: (a: AiAction) => (
              <div>
                  <div className="font-medium text-foreground">{a.name}</div>
                  <div className="text-[10px] text-muted-foreground font-mono">/api/v1/ai/run/{a.slug}</div>
              </div>
          ) 
      },
      { 
          field: 'model', headerName: 'Model', width: '150px',
          renderCell: (a: AiAction) => <Badge variant="outline" className="text-[10px]">{a.model}</Badge>
      },
      {
          field: 'actions', headerName: '', align: 'right' as const, width: '150px',
          renderCell: (a: AiAction) => (
              <div className="flex justify-end gap-1">
                  <Button size="icon" variant="ghost" onClick={(e) => { e.stopPropagation(); setSelectedAction(a); setTesterOpen(true); }} title="Test Run">
                      <Zap className="h-4 w-4 text-primary" />
                  </Button>
                  <Button size="icon" variant="ghost" onClick={(e) => { e.stopPropagation(); setDeleteId(a.id); }}>
                      <Trash2 className="h-4 w-4 text-muted-foreground hover:text-destructive" />
                  </Button>
              </div>
          )
      }
  ];

  return (
    <div className="space-y-6">
        <div className="flex items-center justify-between">
            <div>
                <h2 className="text-3xl font-bold tracking-tight">AI Actions</h2>
                <p className="text-muted-foreground">Manage LLM prompt templates and endpoints.</p>
            </div>
            <Button onClick={() => { setSelectedAction(null); setEditorOpen(true); }}>
                <Plus className="mr-2 h-4 w-4" /> New Action
            </Button>
        </div>

        <DataGrid 
            data={actions} 
            columns={columns} 
            keyField="id" 
            isLoading={isLoading}
        />

        <AiActionEditor 
            isOpen={editorOpen} 
            onClose={() => setEditorOpen(false)} 
            onSave={handleSave}
            initialData={selectedAction || undefined}
        />

        {selectedAction && (
            <AiActionTester 
                action={selectedAction}
                isOpen={testerOpen}
                onClose={() => setTesterOpen(false)}
            />
        )}

        <ConfirmDialog 
            isOpen={!!deleteId}
            title="Delete Action"
            description="Are you sure? Apps using this endpoint will fail."
            onConfirm={handleDelete}
            onCancel={() => setDeleteId(null)}
        />
    </div>
  );
};