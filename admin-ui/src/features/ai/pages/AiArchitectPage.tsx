import React, { useState, useEffect } from 'react';
import { Sparkles, Plus, MessageSquare, Calendar, ArrowRight, Loader2 } from 'lucide-react';
import { Button, Card, CardContent, CardHeader, CardTitle, Input } from '../../../components/ui/Elements';
import { AiSessionPanel } from '../components/AiSessionPanel';
import { architectService, AiSession } from '../services/architectService';
import { useToast } from '../../../components/feedback/Toast';

import { AI_MODELS, DEFAULT_AI_MODEL } from '../../../config/ai-models';
import { Select } from '../../../components/ui/Elements';

export const AiArchitectPage = () => {
    const [sessions, setSessions] = useState<AiSession[]>([]);
    const [activeSession, setActiveSession] = useState<AiSession | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isCreating, setIsCreating] = useState(false);

    const [selectedModel, setSelectedModel] = useState(DEFAULT_AI_MODEL);

    // New Session Form State
    const [newProjectName, setNewProjectName] = useState('');

    const { toast } = useToast();

    const loadSessions = async () => {
        try {
            const data = await architectService.listSessions();
            setSessions(data);
        } catch (e) {
            console.error(e);
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => {
        loadSessions();
    }, []);

    const handleCreate = async () => {
        if (!newProjectName) return;
        setIsCreating(true);
        try {
            const newSession = await architectService.createSession(newProjectName, undefined, selectedModel);
            setSessions([newSession, ...sessions]);
            setActiveSession(newSession); // Open immediately
            setNewProjectName('');
        } catch (e: any) {
            toast(e.message, 'error');
        } finally {
            setIsCreating(false);
        }
    };

    return (
        <div className="space-y-6 pb-20">
            <div className="flex items-center justify-between">
                <div>
                    <h1 className="text-3xl font-bold tracking-tight flex items-center gap-2">
                        <Sparkles className="h-8 w-8 text-primary" /> AI Architect
                    </h1>
                    <p className="text-muted-foreground">Build, iterate, and deploy apps using natural language.</p>
                </div>
            </div>

            {/* Create New Bar */}
            <Card className="bg-gradient-to-r from-primary/10 to-transparent border-primary/20">
                <CardContent className="p-4 flex gap-4 items-center">
                    <div className="h-10 w-10 rounded-full bg-primary/20 flex items-center justify-center shrink-0">
                        <Plus className="h-6 w-6 text-primary" />
                    </div>
                    <div className="flex-1 flex gap-2">
                        <Input
                            placeholder="Project Name (e.g. CRM System, Job Board)..."
                            value={newProjectName}
                            onChange={(e: any) => setNewProjectName(e.target.value)}
                            className="bg-background"
                            onKeyDown={(e: any) => e.key === 'Enter' && handleCreate()}
                        />
                        <div className="w-48">
                            <Select
                                value={selectedModel}
                                onChange={(e: any) => setSelectedModel(e.target.value)}
                            >
                                {AI_MODELS.map(m => <option key={m.value} value={m.value}>{m.label}</option>)}
                            </Select>
                        </div>
                    </div>
                    <Button onClick={handleCreate} isLoading={isCreating} disabled={!newProjectName}>
                        Start Project
                    </Button>
                </CardContent>
            </Card>

            {/* Session Grid */}
            {isLoading ? (
                <div className="flex justify-center p-12"><Loader2 className="animate-spin text-primary" /></div>
            ) : sessions.length === 0 ? (
                <div className="text-center py-12 text-muted-foreground">No projects yet. Start one above!</div>
            ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                    {sessions.map(session => (
                        <span key={session.id} onClick={() => { setActiveSession(session) }}>
                            <Card
                                className="hover:border-primary/50 transition-colors cursor-pointer group"
                            >
                                <CardHeader className="pb-2">
                                    <CardTitle className="text-lg flex justify-between items-start">
                                        <span>{session.name}</span>
                                        <ArrowRight className="h-4 w-4 opacity-0 group-hover:opacity-100 transition-opacity text-primary" />
                                    </CardTitle>
                                </CardHeader>
                                <CardContent>
                                    <div className="flex items-center gap-4 text-xs text-muted-foreground">
                                        <div className="flex items-center gap-1">
                                            <MessageSquare className="h-3 w-3" />
                                            {session.messages.length} msgs
                                        </div>
                                        <div className="flex items-center gap-1">
                                            <Calendar className="h-3 w-3" />
                                            {new Date(session.created_at).toLocaleDateString()}
                                        </div>
                                    </div>
                                    {session.current_manifest && (
                                        <div className="mt-3 flex gap-1 flex-wrap">
                                            <span className="bg-secondary px-1.5 py-0.5 rounded text-[10px] border border-border">
                                                {session.current_manifest.collections.length} Collections
                                            </span>
                                            <span className="bg-secondary px-1.5 py-0.5 rounded text-[10px] border border-border">
                                                {session.current_manifest.templates.length} Pages
                                            </span>
                                        </div>
                                    )}
                                </CardContent>
                            </Card>
                        </span>
                    ))}
                </div>
            )}

            {/* Deep Interaction Panel */}
            <AiSessionPanel
                session={activeSession}
                onClose={() => setActiveSession(null)}
                onUpdate={(updated) => {
                    setSessions(prev => prev.map(s => s.id === updated.id ? updated : s));
                    setActiveSession(updated);
                }}
            />
        </div>
    );
};