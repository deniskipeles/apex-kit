import React, { useState, useEffect, useRef } from 'react';
import {
  Search,
  BrainCircuit,
  Zap,
  RefreshCw,
  ArrowRight,
  Image as ImageIcon,
  Type,
  X,
  UploadCloud,
} from 'lucide-react';
import {
  Button,
  Input,
  Select,
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Badge,
  Label,
  Switch,
} from '../../../components/ui/Elements';
import { collectionsService } from '../../collections/services/collectionsService';
import { Collection, AppRecord } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { apiClient } from '../../../lib/apiClient';

interface VectorSearchResult extends AppRecord {
  _score?: number;
}

export const VectorSearchPanel = () => {
  const [collections, setCollections] = useState<Collection[]>([]);
  const [selectedColId, setSelectedColId] = useState<string>('');
  const [searchType, setSearchType] = useState<'text' | 'image'>('text');
  const [query, setQuery] = useState('');
  const [imageBase64, setImageBase64] = useState<string>('');

  const [results, setResults] = useState<VectorSearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [isReindexing, setIsReindexing] = useState(false);
  const [forceRevectorize, setForceRevectorize] = useState(false);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const { toast } = useToast();

  useEffect(() => {
    collectionsService.list().then((cols) => {
      const vectorCols = cols.filter((c) => c.schema.some((f) => f.vectorize));
      setCollections(vectorCols);
      if (vectorCols.length > 0) setSelectedColId(vectorCols[0].id);
    });
  }, []);

  const selectedCollection = collections.find((c) => c.id === selectedColId);

  const handleImageUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onloadend = () => {
      setImageBase64(reader.result as string);
    };
    reader.readAsDataURL(file);
  };

  const handleSearch = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!selectedColId) return;
    if (searchType === 'text' && !query.trim()) return;
    if (searchType === 'image' && !imageBase64) return;

    setIsSearching(true);
    setResults([]);

    try {
      let data;
      if (searchType === 'text') {
        data = (await apiClient.records.searchVectorWithText(selectedColId, query, { limit: 10 }))
          .items;
      } else {
        data = await apiClient.records.searchImageVectorWithImage(selectedColId, imageBase64, 10);
      }
      setResults(data as any);
    } catch (err: any) {
      toast(err.message, 'error');
    } finally {
      setIsSearching(false);
    }
  };

  const handleRevectorize = async () => {
    if (!selectedColId) return;
    setIsReindexing(true);
    try {
      const res = await apiClient.revectorizeCollection(selectedColId, forceRevectorize);
      if (res.ok || res.success) {
        toast(res.message || 'Background jobs started', 'success');
      } else {
        throw new Error('Failed to start job');
      }
    } catch (e) {
      toast('Re-vectorization failed', 'error');
    } finally {
      setIsReindexing(false);
    }
  };

  return (
    <div className="space-y-6 max-w-5xl mx-auto">
      <div className="flex flex-col md:flex-row gap-6">
        <div className="w-full md:w-1/3 space-y-6">
          <Card className="border-primary/20 bg-gradient-to-b from-primary/5 to-transparent">
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <BrainCircuit className="h-5 w-5 text-primary" /> Vector Search
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">
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
                    {collections.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                  </Select>
                )}
              </div>

              {selectedCollection && (
                <div className="space-y-2">
                  <Label className="text-xs text-muted-foreground uppercase">
                    Vectorized Fields
                  </Label>
                  <div className="flex flex-wrap gap-1">
                    {selectedCollection.schema
                      .filter((f) => f.vectorize)
                      .map((f) => (
                        <Badge key={f.name} variant="secondary" className="font-mono text-[10px]">
                          {f.name} ({f.type})
                        </Badge>
                      ))}
                  </div>
                </div>
              )}

              <div className="space-y-4">
                <div className="flex p-1 bg-secondary/20 rounded-lg">
                  <button
                    type="button"
                    onClick={() => setSearchType('text')}
                    className={`flex-1 flex items-center justify-center gap-2 text-xs py-1.5 rounded-md transition-all ${searchType === 'text' ? 'bg-background shadow-sm font-medium text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                  >
                    <Type className="h-3.5 w-3.5" /> Text Query
                  </button>
                  <button
                    type="button"
                    onClick={() => setSearchType('image')}
                    className={`flex-1 flex items-center justify-center gap-2 text-xs py-1.5 rounded-md transition-all ${searchType === 'image' ? 'bg-background shadow-sm font-medium text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                  >
                    <ImageIcon className="h-3.5 w-3.5" /> Image Query
                  </button>
                </div>

                {searchType === 'text' ? (
                  <div className="space-y-2 animate-in fade-in zoom-in-95 duration-200">
                    <Label>Natural Language Query</Label>
                    <div className="relative">
                      <Input
                        placeholder="e.g. 'Similar to retro sci-fi movies'..."
                        value={query}
                        onChange={(e: any) => setQuery(e.target.value)}
                        className="pr-10"
                        onKeyDown={(e: any) => e.key === 'Enter' && handleSearch()}
                      />
                      <Search className="absolute right-3 top-2.5 h-4 w-4 text-muted-foreground" />
                    </div>
                  </div>
                ) : (
                  <div className="space-y-2 animate-in fade-in zoom-in-95 duration-200">
                    <Label>Image Query</Label>
                    {imageBase64 ? (
                      <div className="relative rounded-lg overflow-hidden border border-border group bg-black/50 flex items-center justify-center h-40">
                        <img
                          src={imageBase64}
                          alt="Query"
                          className="max-h-full max-w-full object-contain"
                        />
                        <button
                          onClick={() => setImageBase64('')}
                          className="absolute top-2 right-2 bg-black/70 text-white p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity"
                        >
                          <X className="h-4 w-4" />
                        </button>
                      </div>
                    ) : (
                      <div
                        className="border-2 border-dashed border-border rounded-lg p-6 flex flex-col items-center justify-center gap-2 text-muted-foreground hover:border-primary/50 hover:bg-secondary/5 transition-colors cursor-pointer h-40"
                        onClick={() => fileInputRef.current?.click()}
                      >
                        <UploadCloud className="h-8 w-8 opacity-50" />
                        <span className="text-xs font-medium">Click to upload image</span>
                        <input
                          type="file"
                          className="hidden"
                          ref={fileInputRef}
                          accept="image/*"
                          onChange={handleImageUpload}
                        />
                      </div>
                    )}
                  </div>
                )}

                <Button
                  type="button"
                  className="w-full"
                  isLoading={isSearching}
                  onClick={() => handleSearch()}
                  disabled={
                    !selectedColId ||
                    (searchType === 'text' && !query) ||
                    (searchType === 'image' && !imageBase64)
                  }
                >
                  <Zap className="mr-2 h-4 w-4" /> Find Matches
                </Button>
              </div>

              <div className="pt-4 border-t border-border space-y-3">
                <div className="flex items-center justify-between p-2 rounded-md bg-secondary/5 border border-border/50">
                  <Label
                    className="text-xs cursor-pointer text-muted-foreground"
                    onClick={() => setForceRevectorize(!forceRevectorize)}
                  >
                    Force Overwrite Existing
                  </Label>
                  <Switch checked={forceRevectorize} onCheckedChange={setForceRevectorize} />
                </div>
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
              </div>
            </CardContent>
          </Card>
        </div>

        <div className="flex-1">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-semibold text-lg">Results ({results.length})</h3>
            {results.length > 0 && (
              <span className="text-xs text-muted-foreground">
                Top {results.length} matches by cosine similarity
              </span>
            )}
          </div>

          <div className="space-y-3">
            {isSearching ? (
              Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="h-24 rounded-lg bg-secondary/20 animate-pulse" />
              ))
            ) : results.length === 0 ? (
              <div className="h-64 border-2 border-dashed border-border rounded-xl flex flex-col items-center justify-center text-muted-foreground">
                <Search className="h-10 w-10 mb-2 opacity-20" />
                <p>Enter a query to find semantically similar records.</p>
              </div>
            ) : (
              results.map((record, i) => {
                const imageFieldKey = Object.keys(record.data).find(
                  (k) =>
                    typeof record.data[k] === 'string' &&
                    record.data[k].match(/\.(jpg|jpeg|png|webp|gif)$/i)
                );
                const imageUrl = imageFieldKey
                  ? apiClient.files.getFileUrl(record.data[imageFieldKey])
                  : null;

                return (
                  <div
                    key={record.id}
                    className="bg-card border border-border rounded-lg p-4 transition-all hover:border-primary/50 hover:shadow-md group flex gap-4"
                  >
                    {imageUrl && (
                      <div className="h-16 w-16 shrink-0 rounded-md overflow-hidden bg-black/10 border border-border">
                        <img
                          src={`${imageUrl}?thumb=100x100`}
                          alt="Thumbnail"
                          className="h-full w-full object-cover"
                        />
                      </div>
                    )}

                    <div className="flex-1 min-w-0">
                      <div className="flex justify-between items-start mb-2">
                        <div className="flex items-center gap-2">
                          <span className="font-mono text-xs text-muted-foreground bg-secondary px-1.5 py-0.5 rounded">
                            #{record.id}
                          </span>
                          <span className="font-medium truncate max-w-[200px]">
                            {record.data.title ||
                              record.data.name ||
                              record.data.email ||
                              'Untitled Record'}
                          </span>
                        </div>
                        <Badge
                          variant="outline"
                          className={`font-mono text-[10px] shrink-0 ${i === 0 ? 'bg-emerald-500/10 text-emerald-500 border-emerald-500/20' : ''}`}
                        >
                          Match #{i + 1}
                        </Badge>
                      </div>

                      <div className="text-sm text-muted-foreground line-clamp-2 mb-3">
                        {Object.entries(record.data)
                          .filter(
                            ([k, v]) =>
                              typeof v === 'string' && v.length > 20 && k !== imageFieldKey
                          )
                          .map(([k, v]) => v)
                          .join(' ... ') || JSON.stringify(record.data)}
                      </div>

                      <div className="flex justify-end opacity-0 group-hover:opacity-100 transition-opacity">
                        <Button size="sm" variant="ghost" className="h-6 text-xs gap-1">
                          View Data <ArrowRight className="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
