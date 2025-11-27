
import React, { useState, useRef } from 'react';
import { ChevronDown, Search, Plus, User, Check } from 'lucide-react';
import { Button, Input, Badge, Card, CardContent, CardHeader, CardTitle } from '../components/ui/Elements';
import { Overlay } from '../components/overlay/Overlay';

const INITIAL_AUTHORS = [
  { id: '1', name: 'Alice Johnson', email: 'alice@example.com' },
  { id: '2', name: 'Bob Smith', email: 'bob@test.io' },
  { id: '3', name: 'Charlie Brown', email: 'charlie@peanuts.com' },
];

export const OverlayDemo = () => {
  const [selectedAuthorId, setSelectedAuthorId] = useState<string | null>(null);
  const [authors, setAuthors] = useState(INITIAL_AUTHORS);
  
  const [isListOpen, setIsListOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const createBtnRef = useRef<HTMLButtonElement>(null);
  const [newAuthorName, setNewAuthorName] = useState('');

  const selectedAuthor = authors.find(a => a.id === selectedAuthorId);
  const filteredAuthors = authors.filter(a => a.name.toLowerCase().includes(searchQuery.toLowerCase()));

  const handleCreate = () => {
    if (!newAuthorName) return;
    const newAuth = { 
      id: Math.random().toString(36).substr(2, 9), 
      name: newAuthorName, 
      email: newAuthorName.toLowerCase().replace(/\s/g, '.') + '@demo.com' 
    };
    setAuthors([...authors, newAuth]);
    setSelectedAuthorId(newAuth.id);
    setNewAuthorName('');
    setIsCreateOpen(false);
    setIsListOpen(false);
  };

  return (
    <div className="p-12 max-w-4xl mx-auto space-y-8">
      <div className="space-y-2">
        <h1 className="text-3xl font-bold tracking-tight">Nested Overlay Interaction</h1>
        <p className="text-muted-foreground">
           Demonstrating the "Foreign Key" selection flow: Click Table Cell → Open List → Click Add → Open Form.
        </p>
      </div>

      <Card className="overflow-visible">
        <CardHeader>
           <CardTitle>Mock Records Table</CardTitle>
        </CardHeader>
        <CardContent>
          <table className="w-full text-sm text-left border-collapse">
            <thead className="bg-secondary/20 text-muted-foreground">
              <tr>
                <th className="px-4 py-3 font-medium border border-border">Post Title</th>
                <th className="px-4 py-3 font-medium border border-border w-[300px]">Author (Foreign Key)</th>
                <th className="px-4 py-3 font-medium border border-border">Status</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td className="px-4 py-3 border border-border">The Future of React</td>
                <td className="px-4 py-3 border border-border">
                    
                    <button
                        ref={triggerRef}
                        onClick={() => setIsListOpen(!isListOpen)}
                        className={`flex items-center justify-between w-full px-3 py-1.5 text-sm border rounded-md hover:bg-accent transition-colors ${isListOpen ? 'ring-2 ring-primary border-primary' : 'border-input'}`}
                    >
                        <div className="flex items-center gap-2">
                            <User className="h-3.5 w-3.5 text-muted-foreground" />
                            <span className={selectedAuthor ? 'text-foreground' : 'text-muted-foreground'}>
                                {selectedAuthor ? selectedAuthor.name : 'Select author...'}
                            </span>
                        </div>
                        <ChevronDown className="h-3 w-3 opacity-50" />
                    </button>

                    <Overlay
                        isOpen={isListOpen}
                        onClose={() => setIsListOpen(false)}
                        anchorRef={triggerRef as React.RefObject<HTMLElement>}
                        width={triggerRef.current?.offsetWidth ?? 'auto'} 
                        className="bg-popover text-popover-foreground shadow-xl border border-border rounded-md flex flex-col min-w-[200px]"
                    >
                        <div className="p-2 border-b border-border">
                            <div className="relative">
                                <Search className="absolute left-2 top-2 h-3.5 w-3.5 text-muted-foreground" />
                                <input 
                                    className="w-full bg-secondary/20 border border-transparent focus:border-primary rounded text-xs h-8 pl-7 pr-2 outline-none"
                                    placeholder="Search authors..."
                                    value={searchQuery}
                                    onChange={(e) => setSearchQuery(e.target.value)}
                                    autoFocus
                                />
                            </div>
                        </div>
                        
                        <div className="max-h-[200px] overflow-y-auto py-1">
                            {filteredAuthors.length === 0 && (
                                <div className="px-2 py-3 text-center text-xs text-muted-foreground">
                                    No authors found.
                                </div>
                            )}
                            {filteredAuthors.map(author => (
                                <button
                                    key={author.id}
                                    onClick={() => {
                                        setSelectedAuthorId(author.id);
                                        setIsListOpen(false);
                                    }}
                                    className="w-full text-left px-3 py-2 text-sm hover:bg-secondary/50 flex items-center justify-between group"
                                >
                                    <div className="flex flex-col">
                                        <span>{author.name}</span>
                                        <span className="text-[10px] text-muted-foreground">{author.email}</span>
                                    </div>
                                    {selectedAuthorId === author.id && <Check className="h-3 w-3 text-primary" />}
                                </button>
                            ))}
                        </div>

                        <div className="p-2 border-t border-border bg-secondary/5">
                            <Button 
                                ref={createBtnRef}
                                size="sm" 
                                variant="secondary" 
                                className="w-full text-xs h-8"
                                onClick={(e: any) => {
                                    e.stopPropagation();
                                    setIsCreateOpen(!isCreateOpen);
                                }}
                            >
                                <Plus className="mr-1.5 h-3 w-3" /> Create New Author
                            </Button>
                        </div>

                        <Overlay
                            isOpen={isCreateOpen}
                            onClose={() => setIsCreateOpen(false)}
                            anchorRef={createBtnRef as React.RefObject<HTMLElement>}
                            width={280}
                            className="bg-background border border-primary/30 shadow-2xl rounded-md flex flex-col"
                        >
                            <div className="p-3 space-y-3">
                                <div className="space-y-1">
                                    <h4 className="text-xs font-semibold uppercase tracking-wider text-primary">New Author</h4>
                                    <p className="text-[10px] text-muted-foreground">Enter details to quick add.</p>
                                </div>
                                <div className="space-y-2">
                                    <Input 
                                        placeholder="Full Name" 
                                        value={newAuthorName}
                                        onChange={(e: any) => setNewAuthorName(e.target.value)}
                                        className="h-8 text-sm"
                                        autoFocus
                                    />
                                </div>
                                <div className="flex justify-end gap-2">
                                    <Button 
                                        size="sm" 
                                        variant="ghost" 
                                        className="h-7 text-xs"
                                        onClick={() => setIsCreateOpen(false)}
                                    >
                                        Cancel
                                    </Button>
                                    <Button 
                                        size="sm" 
                                        className="h-7 text-xs"
                                        onClick={handleCreate}
                                        disabled={!newAuthorName}
                                    >
                                        Save & Select
                                    </Button>
                                </div>
                            </div>
                        </Overlay>

                    </Overlay>
                </td>
                <td className="px-4 py-3 border border-border">
                    <Badge variant="success">Published</Badge>
                </td>
              </tr>
            </tbody>
          </table>
        </CardContent>
      </Card>
    </div>
  );
};
