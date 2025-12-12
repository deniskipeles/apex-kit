import React, { useState } from 'react';
import { Sparkles, ArrowRight, Loader2, X } from 'lucide-react';
import { Button, Input } from '../../../components/ui/Elements';
import { useToast } from '../../../components/feedback/Toast';
import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { architectService } from '../services/architectService';


interface AiCodeAssistantProps {
    currentCode: string;
    contextType: 'script' | 'template';
    onApply: (newCode: string) => void;
}

export const AiCodeAssistant = ({ currentCode, contextType, onApply }: AiCodeAssistantProps) => {
    const [isOpen, setIsOpen] = useState(false);
    const [prompt, setPrompt] = useState('');
    const [isLoading, setIsLoading] = useState(false);
    const [model, setModel] = useState(DEFAULT_AI_MODEL);
    const { toast } = useToast();

    const handleGenerate = async () => {
        if (!prompt.trim()) return;
        setIsLoading(true);
        try {
            const data = await architectService.codeEdit(prompt,currentCode,contextType,model);
            onApply(data.code);
            setIsOpen(false);
            setPrompt('');
            toast('Code updated by AI', 'success');
        } catch (e) {
            toast('Failed to generate code', 'error');
        } finally {
            setIsLoading(false);
        }
    };

    if (!isOpen) {
        return (
            <Button
                variant="ghost"
                size="sm"
                className="text-xs text-purple-400 hover:text-purple-300 hover:bg-purple-500/10 gap-2"
                onClick={() => setIsOpen(true)}
            >
                <Sparkles className="h-3 w-3" /> AI Edit
            </Button>
        );
    }

    return (
        <div className="flex flex-col gap-2 w-full animate-in fade-in slide-in-from-top-1 duration-200 bg-purple-500/10 p-2 rounded-md border border-purple-500/30">

            <div className="flex items-center gap-2">
                <Sparkles className="h-4 w-4 text-purple-400 shrink-0 ml-1" />
                <Input
                    value={prompt}
                    onChange={(e: any) => setPrompt(e.target.value)}
                    placeholder={contextType === 'script' ? "e.g. Refactor to use try/catch..." : "e.g. Change buttons to blue..."}
                    className="h-7 text-xs border-0 focus-visible:ring-0 bg-transparent placeholder:text-purple-300/50"
                    autoFocus
                    onKeyDown={(e: any) => e.key === 'Enter' && handleGenerate()}
                    disabled={isLoading}
                />
                <button onClick={() => setIsOpen(false)}><X className="h-3.5 w-3.5" /></button>
            </div>

            <div className="flex items-center justify-between pl-1">
                <select
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                    className="bg-transparent text-[10px] text-purple-300 border-none focus:ring-0 cursor-pointer w-40"
                >
                    {AI_MODELS.map(m => <option key={m.value} value={m.value} className="bg-[#1e1e1e]">{m.label}</option>)}
                </select>

                <Button size="sm" className="h-6 text-xs bg-purple-600" onClick={handleGenerate} disabled={isLoading}>
                    {isLoading ? <Loader2 className="h-3 w-3 animate-spin" /> : 'Generate'}
                </Button>
            </div>
        </div>
    );
};