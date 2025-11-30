import React, { useState, useEffect } from 'react';
import { Play, Loader2, Braces, CornerDownRight } from 'lucide-react';
import { Button, Input, Label } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { AiAction } from '../../../types';
import { aiService } from '../services/aiService';

interface AiActionTesterProps {
  action: AiAction;
  isOpen: boolean;
  onClose: () => void;
}

export const AiActionTester = ({ action, isOpen, onClose }: AiActionTesterProps) => {
  const [variables, setVariables] = useState<Record<string, string>>({});
  const [result, setResult] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [fields, setFields] = useState<string[]>([]);

  // Parse template for {{vars}}
  useEffect(() => {
    if (action) {
        const regex = /\{\{(\w+)\}\}/g;
        const matches = [...action.template.matchAll(regex)].map(m => m[1]);
        const uniqueFields = [...new Set(matches)];
        setFields(uniqueFields);
        
        // Init state
        const initial: Record<string, string> = {};
        uniqueFields.forEach(f => initial[f] = '');
        setVariables(initial);
        setResult(null);
    }
  }, [action, isOpen]);

  const handleRun = async () => {
    setIsLoading(true);
    setResult(null);
    try {
        const res = await aiService.run(action.slug, variables);
        setResult(res.result || JSON.stringify(res, null, 2));
    } catch (e: any) {
        setResult(`Error: ${e.message || 'Unknown error'}`);
    } finally {
        setIsLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title={`Test: ${action.name}`} size="lg">
       <div className="grid grid-cols-1 md:grid-cols-2 gap-6 min-h-[400px]">
           
           {/* Inputs */}
           <div className="space-y-4">
               <div className="text-xs font-bold uppercase text-muted-foreground tracking-wider mb-2">Input Variables</div>
               {fields.length === 0 ? (
                   <div className="p-4 bg-secondary/10 rounded-lg text-sm text-muted-foreground italic border border-border border-dashed">
                       No variables found in template. Just run it.
                   </div>
               ) : (
                   fields.map(field => (
                       <div key={field} className="space-y-1.5">
                           <Label className="font-mono text-xs text-primary">{field}</Label>
                           <textarea 
                               className="flex min-h-[60px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                               value={variables[field] || ''}
                               onChange={(e) => setVariables({...variables, [field]: e.target.value})}
                               placeholder={`Value for ${field}...`}
                           />
                       </div>
                   ))
               )}
               
               <div className="pt-4">
                   <Button className="w-full" onClick={handleRun} isLoading={isLoading}>
                       <Play className="mr-2 h-4 w-4" /> Run Action
                   </Button>
               </div>
           </div>

           {/* Output */}
           <div className="flex flex-col h-full">
               <div className="text-xs font-bold uppercase text-muted-foreground tracking-wider mb-2">Output</div>
               <div className="flex-1 bg-[#1e1e1e] rounded-lg border border-border p-4 font-mono text-sm text-[#d4d4d4] overflow-auto shadow-inner">
                   {isLoading ? (
                       <div className="flex items-center justify-center h-full text-primary gap-2">
                           <Loader2 className="animate-spin h-5 w-5" /> Generating...
                       </div>
                   ) : result ? (
                       <div className="whitespace-pre-wrap animate-in fade-in">{result}</div>
                   ) : (
                       <div className="flex flex-col items-center justify-center h-full text-muted-foreground/30 gap-2">
                           <Braces className="h-8 w-8" />
                           <span>Results will appear here</span>
                       </div>
                   )}
               </div>
           </div>
       </div>
    </Dialog>
  );
};