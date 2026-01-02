import React, { useState, useEffect } from 'react';
import { Save, Sparkles, Info } from 'lucide-react';
import { Button, Input, Label, Select } from '../../../components/ui/Elements';
import { Dialog } from '../../../components/ui/Dialog';
import { AiAction } from '../../../types';

interface AiActionEditorProps {
    isOpen: boolean;
    onClose: () => void;
    onSave: (data: Partial<AiAction>) => Promise<void>;
    initialData?: AiAction;
}

// Updated Model List based on your supported models
const MODEL_GROUPS = [
    {
        label: "Gemini Flash (Fast & Efficient)",
        models: [
            { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash' },
            { id: 'gemini-2.5-flash-lite', name: 'Gemini 2.5 Flash Lite' },
            { id: 'gemini-2.0-flash', name: 'Gemini 2.0 Flash' },
            { id: 'gemini-2.0-flash-lite', name: 'Gemini 2.0 Flash Lite' },
        ]
    },
    {
        "label": "Image Generation & Editing",
        "models": [
            {
                "id": "gemini-3-pro-image-preview",
                "name": "Gemini 3 Pro Image Preview"
            },
            {
                "id": "gemini-2.5-flash-image",
                "name": "Gemini 2.5 Flash Image"
            },
            {
                "id": "imagen-4.0-generate-001",
                "name": "Imagen 4"
            },
            {
                "id": "imagen-4.0-ultra-generate-001",
                "name": "Imagen 4 Ultra"
            },
            {
                "id": "imagen-4.0-fast-generate-001",
                "name": "Imagen 4 Fast"
            }
        ]
    },
    {
        label: "Gemini Pro (High Intelligence)",
        models: [
            { id: 'gemini-3-pro', name: 'Gemini 3 Pro' },
            { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro' },
        ]
    },
    {
        label: "Gemma 3 (Open Models)",
        models: [
            { id: 'gemma-3-27b', name: 'Gemma 3 (27B)' },
            { id: 'gemma-3-12b', name: 'Gemma 3 (12B)' },
            { id: 'gemma-3-4b', name: 'Gemma 3 (4B)' },
            { id: 'gemma-3-2b', name: 'Gemma 3 (2B)' },
            { id: 'gemma-3-1b', name: 'Gemma 3 (1B)' },
        ]
    },
    {
        label: "Specialized & Experimental",
        models: [
            { id: 'gemini-2.5-flash-tts', name: 'Gemini 2.5 Flash TTS (Multimodal)' },
            { id: 'learnlm-2.0-flash-experimental', name: 'LearnLM 2.0 Flash (Experimental)' },
            { id: 'gemini-robotics-er-1.5-preview', name: 'Gemini Robotics 1.5 (Preview)' },
            { id: 'gemini-2.0-flash-exp', name: 'Gemini 2.0 Flash (Experimental)' },
        ]
    },
    {
        label: "Live API (Real-time)",
        models: [
            { id: 'gemini-2.5-flash-live', name: 'Gemini 2.5 Flash Live' },
            { id: 'gemini-2.0-flash-live', name: 'Gemini 2.0 Flash Live' },
            { id: 'gemini-2.5-flash-native-audio-dialog', name: 'Gemini 2.5 Native Audio' },
        ]
    }
];

export const AiActionEditor = ({ isOpen, onClose, onSave, initialData }: AiActionEditorProps) => {
    const [formData, setFormData] = useState<Partial<AiAction>>({
        name: '',
        slug: '',
        model: 'gemini-2.5-flash', // Default to latest stable flash
        system_prompt: '',
        template: 'User Request: {{input}}\n\nContext: ...'
    });
    const [isSaving, setIsSaving] = useState(false);

    useEffect(() => {
        if (initialData) {
            setFormData(initialData);
        } else {
            // Reset defaults on new open
            setFormData({
                name: '',
                slug: '',
                model: 'gemini-2.5-flash',
                system_prompt: '',
                template: 'User Request: {{input}}'
            });
        }
    }, [initialData, isOpen]);

    const handleSave = async () => {
        setIsSaving(true);
        try {
            await onSave(formData);
            onClose();
        } finally {
            setIsSaving(false);
        }
    };

    // Auto-slug generator
    const handleNameChange = (val: string) => {
        if (!initialData) {
            const slug = val.toLowerCase()
                .replace(/[^a-z0-9]+/g, '-')
                .replace(/(^-|-$)+/g, '');
            setFormData(prev => ({ ...prev, name: val, slug }));
        } else {
            setFormData(prev => ({ ...prev, name: val }));
        }
    };

    if (!isOpen) return null;

    return (
        <Dialog isOpen={isOpen} onClose={onClose} title={initialData ? 'Edit AI Action' : 'New AI Action'} size="lg">
            <div className="flex flex-col h-[75vh] gap-6">

                {/* Identity Section */}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div className="space-y-2">
                        <Label required>Action Name</Label>
                        <Input
                            value={formData.name}
                            onChange={(e: any) => handleNameChange(e.target.value)}
                            placeholder="e.g. Summarize Text"
                            autoFocus
                        />
                    </div>
                    <div className="space-y-2">
                        <Label required>Slug (API Endpoint)</Label>
                        <div className="flex items-center">
                            <span className="text-xs text-muted-foreground mr-1 bg-secondary/50 px-2 py-1.5 rounded-l border border-r-0 border-input">/ai/run/</span>
                            <Input
                                value={formData.slug}
                                onChange={(e: any) => setFormData({ ...formData, slug: e.target.value })}
                                className="font-mono rounded-l-none"
                                disabled={!!initialData}
                            />
                        </div>
                    </div>
                </div>

                {/* Model Configuration */}
                <div className="space-y-2">
                    <Label required>AI Model</Label>
                    <Select
                        value={formData.model}
                        onChange={(e: any) => setFormData({ ...formData, model: e.target.value })}
                    >
                        {MODEL_GROUPS.map(group => (
                            <optgroup key={group.label} label={group.label}>
                                {group.models.map(m => (
                                    <option key={m.id} value={m.id}>{m.name}</option>
                                ))}
                            </optgroup>
                        ))}
                    </Select>
                    <p className="text-[10px] text-muted-foreground">Select the model best suited for speed, reasoning, or specialized tasks.</p>
                </div>

                {/* System Instruction */}
                <div className="space-y-2">
                    <Label>System Persona / Instructions</Label>
                    <Input
                        value={formData.system_prompt || ''}
                        onChange={(e: any) => setFormData({ ...formData, system_prompt: e.target.value })}
                        placeholder="You are a helpful assistant. You always respond in JSON..."
                    />
                    <p className="text-[10px] text-muted-foreground">Defines the AI's behavior and constraints globally.</p>
                </div>

                {/* Template Editor */}
                <div className="flex-1 flex flex-col space-y-2 min-h-[200px]">
                    <div className="flex justify-between items-center">
                        <Label className="flex items-center gap-2">
                            <Sparkles className="h-3.5 w-3.5 text-primary" /> User Prompt Template
                        </Label>
                        <span className="text-[10px] bg-primary/10 text-primary px-2 py-0.5 rounded">Supports Handlebars: {'{{variable}}'}</span>
                    </div>
                    <div className="flex-1 relative rounded-md border border-input overflow-hidden">
                        <textarea
                            className="absolute inset-0 w-full h-full bg-[#1e1e1e] text-[#d4d4d4] font-mono text-sm p-4 focus:outline-none resize-none leading-relaxed"
                            value={formData.template}
                            onChange={(e) => setFormData({ ...formData, template: e.target.value })}
                            placeholder="Analyze the following text: {{text}}"
                            spellCheck={false}
                        />
                    </div>
                </div>

                {/* Footer */}
                <div className="flex items-center justify-between pt-4 border-t border-border mt-auto shrink-0">
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <Info className="h-4 w-4" />
                        <span>Variables in template automatically become API inputs.</span>
                    </div>
                    <div className="flex gap-3">
                        <Button variant="ghost" onClick={onClose}>Cancel</Button>
                        <Button onClick={handleSave} isLoading={isSaving} disabled={!formData.name || !formData.slug || !formData.template}>
                            <Save className="mr-2 h-4 w-4" /> Save Action
                        </Button>
                    </div>
                </div>
            </div>
        </Dialog>
    );
};