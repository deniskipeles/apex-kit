import React, { useState, useEffect } from 'react';
import { Search, BrainCircuit, Zap, RefreshCw, AlertCircle, ArrowRight } from 'lucide-react';
import { Button, Input, Select, Card, CardHeader, CardTitle, CardContent, Badge, Label } from '../../../components/ui/Elements';
import { collectionsService } from '../../collections/services/collectionsService';
import { Collection, AppRecord } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '@/src/lib/apiClient';

interface VectorSearchResult extends AppRecord {
    _score?: number; // Depending on how you map it in backend, or we might need to adjust backend to return score explicitly in body
}

export const VectorSearchPanel = () => {
    const [collections, setCollections] = useState<Collection[]>([]);
    const [selectedColId, setSelectedColId] = useState<string>('');
    const [query, setQuery] = useState('');
    const [results, setResults] = useState<VectorSearchResult[]>([]);
    const [isSearching, setIsSearching] = useState(false);
    const [isReindexing, setIsReindexing] = useState(false);
    
    const { toast } = useToast();

    // 1. Load Collections
    useEffect(() => {
        collectionsService.list().then(cols => {
            // Filter only collections that have at least one vector field
            const vectorCols = cols.filter(c => 
                c.schema.some(f => f.vectorize)
            );
            setCollections(vectorCols);
            if (vectorCols.length > 0) setSelectedColId(vectorCols[0].id);
        });
    }, []);

    const selectedCollection = collections.find(c => c.id === selectedColId);

    // 2. Handle Search
    const handleSearch = async (e?: React.FormEvent) => {
        e?.preventDefault();
        if (!selectedColId || !query.trim()) return;

        setIsSearching(true);
        setResults([]);

        try {
            // Raw fetch because this might be a new endpoint not in typed SDK yet
            // Or assume apiClient.records.searchTextVector exists
            const data = await apiClient.records.searchTextVector(selectedColId, query, 10);
            setResults(data);
        } catch (err: any) {
            toast(err.message, 'error');
        } finally {
            setIsSearching(false);
        }
    };

    // 3. Handle Re-Vectorize
    const handleRevectorize = async () => {
        if (!selectedColId) return;
        setIsReindexing(true);
        try {
            const res = await apiClient.collections.revectorize(selectedColId);
            if (res.ok) {
                toast(res.message || 'Background jobs started', 'success');
            } else {
                throw new Error("Failed to start job");
            }
        } catch (e) {
            toast("Re-vectorization failed", "error");
        } finally {
            setIsReindexing(false);
        }
    };

    return (
        <div className="space-y-6 max-w-5xl mx-auto">
            <div className="flex flex-col md:flex-row gap-6">
                
                {/* CONTROL PANEL */}
                <div className="w-full md:w-1/3 space-y-6">
                    <Card className="border-primary/20 bg-gradient-to-b from-primary/5 to-transparent">
                        <CardHeader>
                            <CardTitle className="flex items-center gap-2">
                                <BrainCircuit className="h-5 w-5 text-primary" /> Vector Search
                            </CardTitle>
                        </CardHeader>
                        <CardContent className="space-y-4">
                            <div className="space-y-2">
                                <Label>Target Collection</Label>
                                {collections.length === 0 ? (
                                    <div className="p-3 border border-dashed border-border rounded text-xs text-muted-foreground bg-background">
                                        No collections with <code>vectorize: true</code> fields found.
                                    </div>
                                ) : (
                                    <Select 
                                        value={selectedColId} 
                                        onChange={(e: any) => setSelectedColId(e.target.value)}
                                    >
                                        {collections.map(c => (
                                            <option key={c.id} value={c.id}>{c.name}</option>
                                        ))}
                                    </Select>
                                )}
                            </div>

                            {selectedCollection && (
                                <div className="space-y-2">
                                    <Label className="text-xs text-muted-foreground uppercase">Vectorized Fields</Label>
                                    <div className="flex flex-wrap gap-1">
                                        {selectedCollection.schema.filter(f => f.vectorize).map(f => (
                                            <Badge key={f.name} variant="secondary" className="font-mono text-[10px]">
                                                {f.name}
                                            </Badge>
                                        ))}
                                    </div>
                                </div>
                            )}

                            <form onSubmit={handleSearch} className="space-y-4">
                                <div className="space-y-2">
                                    <Label>Natural Language Query</Label>
                                    <div className="relative">
                                        <Input 
                                            placeholder="e.g. 'Similar to retro sci-fi movies'..." 
                                            value={query}
                                            onChange={(e: any) => setQuery(e.target.value)}
                                            className="pr-10"
                                        />
                                        <Search className="absolute right-3 top-2.5 h-4 w-4 text-muted-foreground" />
                                    </div>
                                </div>

                                <Button 
                                    type="submit" 
                                    className="w-full" 
                                    isLoading={isSearching}
                                    disabled={!selectedColId || !query}
                                >
                                    <Zap className="mr-2 h-4 w-4" /> Find Matches
                                </Button>
                            </form>

                            <div className="pt-4 border-t border-border">
                                <Button 
                                    variant="outline" 
                                    size="sm" 
                                    className="w-full text-xs" 
                                    onClick={handleRevectorize}
                                    isLoading={isReindexing}
                                    disabled={!selectedColId}
                                >
                                    <RefreshCw className="mr-2 h-3 w-3" /> Re-generate Embeddings
                                </Button>
                                <p className="text-[10px] text-muted-foreground mt-2 text-center">
                                    Run this if you added data before enabling vectors.
                                </p>
                            </div>
                        </CardContent>
                    </Card>
                </div>

                {/* RESULTS PANEL */}
                <div className="flex-1">
                    <div className="flex items-center justify-between mb-4">
                        <h3 className="font-semibold text-lg">Results ({results.length})</h3>
                        {results.length > 0 && <span className="text-xs text-muted-foreground">Top 10 matches by cosine similarity</span>}
                    </div>

                    <div className="space-y-3">
                        {isSearching ? (
                             Array.from({length: 3}).map((_, i) => (
                                 <div key={i} className="h-24 rounded-lg bg-secondary/20 animate-pulse" />
                             ))
                        ) : results.length === 0 ? (
                            <div className="h-64 border-2 border-dashed border-border rounded-xl flex flex-col items-center justify-center text-muted-foreground">
                                <Search className="h-10 w-10 mb-2 opacity-20" />
                                <p>Enter a query to find semantically similar records.</p>
                            </div>
                        ) : (
                            results.map((record, i) => (
                                <div key={record.id} className="bg-card border border-border rounded-lg p-4 transition-all hover:border-primary/50 hover:shadow-md group">
                                    <div className="flex justify-between items-start mb-2">
                                        <div className="flex items-center gap-2">
                                            <span className="font-mono text-xs text-muted-foreground bg-secondary px-1.5 py-0.5 rounded">#{record.id}</span>
                                            <span className="font-medium truncate max-w-[200px]">
                                                {/* Try to find a sensible title field */}
                                                {record.data.title || record.data.name || record.data.email || 'Untitled Record'}
                                            </span>
                                        </div>
                                        {/* Score Visualizer (Mocked slightly if backend doesn't send explicit score in body yet, usually it's metadata) */}
                                        <Badge variant="outline" className={`font-mono text-[10px] ${i === 0 ? 'bg-emerald-500/10 text-emerald-500 border-emerald-500/20' : ''}`}>
                                            Match #{i+1}
                                        </Badge>
                                    </div>
                                    
                                    {/* Snippet / Content Preview */}
                                    <div className="text-sm text-muted-foreground line-clamp-2 mb-3">
                                        {Object.entries(record.data)
                                            .filter(([k, v]) => typeof v === 'string' && v.length > 20)
                                            .map(([k, v]) => v)
                                            .join(' ... ') || JSON.stringify(record.data)
                                        }
                                    </div>

                                    <div className="flex justify-end opacity-0 group-hover:opacity-100 transition-opacity">
                                        <Button size="sm" variant="ghost" className="h-6 text-xs gap-1">
                                            View Data <ArrowRight className="h-3 w-3" />
                                        </Button>
                                    </div>
                                </div>
                            ))
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
};