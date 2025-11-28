import React, { useState } from 'react';
import { Play, Loader2, AlertTriangle, CheckCircle } from 'lucide-react';
import { Button } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { JSONEditor } from '../../../components/form/JsonEditor';
import { scriptsService } from '../services/scriptsService';
import { Script } from '../../../types';

interface ScriptTesterProps {
  script: Script;
  isOpen: boolean;
  onClose: () => void;
}

export const ScriptTester = ({ script, isOpen, onClose }: ScriptTesterProps) => {
  const [inputJson, setInputJson] = useState('{\n  "name": "Developer"\n}');
  const [output, setOutput] = useState<any>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState('');

  const handleRun = async () => {
    setIsLoading(true);
    setError('');
    setOutput(null);
    
    try {
        const payload = JSON.parse(inputJson);
        const result = await scriptsService.run(script.name, payload);
        setOutput(result);
    } catch (e: any) {
        setError(e.message || 'Execution failed');
    } finally {
        setIsLoading(false);
    }
  };

  if (!isOpen) return null;

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title={`Test Run: ${script.name}`} size="lg">
       <div className="grid grid-cols-1 md:grid-cols-2 gap-6 h-[500px]">
           {/* Input Column */}
           <div className="flex flex-col gap-2">
               <div className="font-semibold text-sm text-muted-foreground uppercase tracking-wider">Input (JSON)</div>
               <div className="flex-1 border border-border rounded-md overflow-hidden">
                   <JSONEditor value={inputJson} onChange={setInputJson} height="100%" />
               </div>
           </div>

           {/* Output Column */}
           <div className="flex flex-col gap-2">
               <div className="font-semibold text-sm text-muted-foreground uppercase tracking-wider">Result</div>
               <div className="flex-1 bg-[#1e1e1e] rounded-md border border-border p-4 font-mono text-sm overflow-auto">
                   {isLoading ? (
                       <div className="flex items-center justify-center h-full text-primary">
                           <Loader2 className="animate-spin h-6 w-6 mr-2" /> Running...
                       </div>
                   ) : error ? (
                       <div className="text-destructive flex items-start gap-2">
                           <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" />
                           <pre className="whitespace-pre-wrap">{error}</pre>
                       </div>
                   ) : output !== null ? (
                       <div className="text-emerald-400">
                           <div className="flex items-center gap-2 mb-2 pb-2 border-b border-white/10 text-xs">
                               <CheckCircle className="h-3 w-3" /> Status: 200 OK
                           </div>
                           <pre className="whitespace-pre-wrap">{JSON.stringify(output, null, 2)}</pre>
                       </div>
                   ) : (
                       <div className="text-muted-foreground/50 italic text-center mt-20">
                           Press Run to execute script
                       </div>
                   )}
               </div>
           </div>
       </div>

       <div className="flex justify-end pt-4 mt-4 border-t border-border">
           <Button onClick={handleRun} disabled={isLoading}>
               <Play className="mr-2 h-4 w-4" /> Run Script
           </Button>
       </div>
    </Dialog>
  );
};