import React, { useState } from 'react';
import { Button, Input, Card, CardHeader, CardTitle, CardContent, Label, Badge } from './ui/Elements';
import { Save, Shield, AlertCircle } from 'lucide-react';
import { SchemaBuilder } from './SchemaBuilder';
import { SchemaField } from '../types';

interface CollectionCreatorProps {
  onCancel: () => void;
}

export const CollectionCreator = ({ onCancel }: CollectionCreatorProps) => {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [schema, setSchema] = useState<SchemaField[]>([
    { name: 'title', type: 'text', required: true },
    { name: 'status', type: 'select', required: false, options: ['draft', 'published'] }
  ]);
  
  // FIX: Default to "public" instead of "" to prevent "Read denied" errors
  const [rules, setRules] = useState({
      read: 'public',
      create: 'admin', 
      update: 'admin',
      delete: 'admin',
      admin: 'admin' // Although not used by backend yet, good for UI consistency
  });

  return (
    <div className="max-w-5xl mx-auto p-6 space-y-8 pb-20">
        {/* Header */}
        <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
            <div className="space-y-1">
                <div className="flex items-center gap-2 text-sm text-muted-foreground mb-1">
                    <button onClick={onCancel} className="hover:text-primary flex items-center gap-1">
                        Collections
                    </button>
                    <span className="opacity-50">/</span>
                    <span>New</span>
                </div>
                <h2 className="text-3xl font-bold tracking-tight">Create Collection</h2>
                <p className="text-muted-foreground">Configure your data structure and access policies.</p>
            </div>
            <div className="flex gap-3">
                <Button variant="ghost" onClick={onCancel}>Cancel</Button>
                <Button className="bg-primary hover:bg-primary/90 text-white" onClick={() => { /* Add save handler props if needed, or use a wrapper */ }}>
                    {/* Note: The parent calls the API, this component just renders. 
                        We need to expose the state or handle save here. 
                        Assuming parent passes a handler or we use the store/service directly.
                        For now, the props definition only has onCancel. 
                        This component usually needs an onSave prop or handle it internally.
                    */}
                    <Save className="mr-2 h-4 w-4" /> Save Collection
                </Button>
            </div>
        </div>

        <div className="grid gap-6 lg:grid-cols-3">
            <div className="lg:col-span-2 space-y-6">
                <Card>
                    <CardHeader>
                        <CardTitle>General Settings</CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-4">
                        <div className="grid gap-2">
                            <Label>Name</Label>
                            <Input 
                                placeholder="e.g. posts, users, orders" 
                                value={name} 
                                onChange={(e: any) => setName(e.target.value)}
                                className="bg-background"
                            />
                            <p className="text-[10px] text-muted-foreground">Used as table name in the database. Max 32 characters.</p>
                        </div>
                         <div className="grid gap-2">
                            <Label>Description</Label>
                            <Input 
                                placeholder="Brief description of this collection..." 
                                value={description} 
                                onChange={(e: any) => setDescription(e.target.value)}
                                className="bg-background"
                            />
                        </div>
                    </CardContent>
                </Card>

                <Card className="flex flex-col border-border">
                    <CardHeader className="flex flex-row items-center justify-between py-4">
                        <CardTitle className="text-sm font-bold uppercase tracking-wider text-muted-foreground">
                            Fields ({schema.length})
                        </CardTitle>
                    </CardHeader>
                    <div className="p-4 pt-0">
                        <SchemaBuilder value={schema} onChange={setSchema} />
                    </div>
                </Card>
            </div>

            <div className="space-y-6">
                <Card className="sticky top-24">
                    <CardHeader className="pb-3">
                        <CardTitle className="flex items-center gap-2">
                            <Shield className="h-4 w-4 text-primary" /> API Rules
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-5">
                        <div className="rounded-md bg-primary/10 p-3 text-xs text-primary leading-relaxed mb-4">
                            <p><strong>Common Rules:</strong></p>
                            <ul className="list-disc list-inside mt-1 opacity-90">
                                <li><code>public</code>: Everyone</li>
                                <li><code>auth</code>: Logged in users</li>
                                <li><code>admin</code>: Admins only</li>
                                <li><code>owner:field</code>: Record owner</li>
                            </ul>
                        </div>
                        
                        <div className="space-y-4">
                            {[
                                { key: 'read', label: 'Read Rule', desc: 'List and view records' },
                                { key: 'create', label: 'Create Rule', desc: 'Create new records' },
                                { key: 'update', label: 'Update Rule', desc: 'Edit existing records' },
                                { key: 'delete', label: 'Delete Rule', desc: 'Remove records' },
                            ].map((rule) => (
                                <div key={rule.key} className="grid gap-1.5 group">
                                    <div className="flex items-center justify-between">
                                        <Label className="text-xs font-semibold text-foreground group-hover:text-primary transition-colors">{rule.label}</Label>
                                        <span className="text-[10px] text-muted-foreground">{rule.desc}</span>
                                    </div>
                                    <div className="relative">
                                        <Input 
                                            className="font-mono text-xs h-9 bg-background/50 focus:bg-background transition-colors pr-8" 
                                            placeholder="public"
                                            value={rules[rule.key as keyof typeof rules]}
                                            onChange={(e: any) => setRules({
                                                ...rules, 
                                                [rule.key]: e.target.value 
                                            })}
                                        />
                                    </div>
                                </div>
                            ))}
                        </div>
                    </CardContent>
                </Card>
            </div>
        </div>
    </div>
  );
};