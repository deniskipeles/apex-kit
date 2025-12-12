import React, { useState, useEffect, useRef } from 'react';
import { Search, Loader2, FileText, X } from 'lucide-react';
import { Input } from '../form/FormPrimitives';
import { recordsService } from '../../features/records/services/recordsService';
import { InstantResult } from '../../types';
import { apiClient } from '@/src/lib/apiClient';

interface InstantSearchInputProps {
  collectionId: string;
  onSelect: (recordId: string) => void;
  placeholder?: string;
}

export const InstantSearchInput = ({ collectionId, onSelect, placeholder = "Instant Search..." }: InstantSearchInputProps) => {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<InstantResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [isOpen, setIsOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  // Debounce Logic: Only search 500ms after typing stops
  useEffect(() => {
    const timer = setTimeout(async () => {
      if (query.trim().length > 2 && collectionId) {
        setLoading(true);
        const hits = await recordsService.instantSearch(collectionId, query);
        setResults(hits);
        setLoading(false);
        setIsOpen(true);
      } else {
        setResults([]);
        setIsOpen(false);
      }
    }, 500);

    return () => clearTimeout(timer);
  }, [query, collectionId]);

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  return (
    <div className="relative w-full sm:w-64" ref={wrapperRef}>
      <div className="relative">
        <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground z-10" />
        <Input
          placeholder={placeholder}
          className="pl-9 pr-8 bg-background/80 backdrop-blur focus:bg-background transition-all"
          value={query}
          onChange={(e: any) => setQuery(e.target.value)}
          onFocus={() => { if (results.length > 0) setIsOpen(true); }}
        />
        {loading ? (
          <Loader2 className="absolute right-2.5 top-2.5 h-4 w-4 animate-spin text-primary" />
        ) : query && (
          <button
            onClick={() => { setQuery(''); setResults([]); setIsOpen(false); }}
            className="absolute right-2.5 top-2.5 text-muted-foreground hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      {/* Results Dropdown */}
      {isOpen && (
        <div className="absolute top-full mt-2 w-full md:w-80 right-0 bg-popover border border-border rounded-lg shadow-xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
          <div className="p-2 border-b border-border bg-secondary/10 text-[10px] font-semibold text-muted-foreground uppercase tracking-wider flex justify-between">
            <span>Tantivy Index Results</span>
            <span>{results.length} Hits</span>
          </div>

          <div className="max-h-[300px] overflow-y-auto z-index-100">
            {results.length === 0 ? (
              <div className="p-4 text-center text-sm text-muted-foreground">
                No index matches found.
              </div>
            ) : (
              results.map((res) => (
                <button
                  key={res.id}
                  onClick={() => {
                    onSelect(res.id.toString());
                    setIsOpen(false);
                    setQuery(''); // Clear search on select
                  }}
                  className="w-full text-left p-3 hover:bg-accent transition-colors flex items-start gap-3 border-b border-border/50 last:border-0"
                >
                  <div className="bg-primary/10 p-2 rounded text-primary mt-0.5">
                    <FileText className="h-4 w-4" />
                  </div>
                  <div className="flex-1 min-w-0">
                    {/* Display the first non-empty string field from snippet as title */}
                    <div className="font-medium text-sm truncate text-foreground">
                      {apiClient.stripHtmlTags(Object.values(res.snippet).find(v => typeof v === 'string')) || `Record #${res.id}`}
                    </div>
                    {/* Display ID and Score */}
                    <div className="flex items-center gap-2 mt-1">
                      <code className="text-[10px] bg-secondary px-1 rounded text-muted-foreground">#{res.id}</code>
                      <span className="text-[10px] text-emerald-500 font-mono">Score: {res.score.toFixed(2)}</span>
                    </div>
                    {/* Debug Snippet content */}
                    <div className="text-[10px] text-muted-foreground mt-1 truncate opacity-70">
                      {apiClient.stripHtmlTags(JSON.stringify(res.snippet) || '').slice(0, 50)}...
                    </div>
                  </div>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
};
