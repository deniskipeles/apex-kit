import React from 'react';
import { Dialog } from '../ui/Dialog';
import { AppVersions } from '../../types';
import { Box, Layers, Cpu, Database, CheckCircle2 } from 'lucide-react';
import { Badge } from '../ui/Elements';

interface VersionsModalProps {
  isOpen: boolean;
  onClose: () => void;
  versions: AppVersions | null;
}

export const VersionsModal = ({ isOpen, onClose, versions }: VersionsModalProps) => {
  if (!versions) return null;

  const items = [
    { label: 'System Root', version: versions.root, icon: Box, desc: 'Main binary version' },
    { label: 'API Layer', version: versions.api, icon: Layers, desc: 'HTTP & Routing logic' },
    { label: 'Core Engine', version: versions.core, icon: Database, desc: 'Database & Auth logic' },
    { label: 'Vector Engine', version: versions.vector, icon: Cpu, desc: 'AI & Embedding logic' },
  ];

  return (
    <Dialog isOpen={isOpen} onClose={onClose} title="System Versions" size="sm">
      <div className="space-y-4">
        <div className="grid gap-3">
          {items.map((item) => (
            <div
              key={item.label}
              className="flex items-center justify-between p-3 rounded-lg border border-border bg-card hover:bg-secondary/5 transition-colors"
            >
              <div className="flex items-center gap-3">
                <div className="h-8 w-8 rounded-full bg-primary/10 flex items-center justify-center text-primary">
                  <item.icon className="h-4 w-4" />
                </div>
                <div>
                  <div className="font-medium text-sm">{item.label}</div>
                  <div className="text-[10px] text-muted-foreground">{item.desc}</div>
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="font-mono text-xs">
                  v{item.version}
                </Badge>
                <CheckCircle2 className="h-3 w-3 text-emerald-500" />
              </div>
            </div>
          ))}
        </div>

        <div className="text-center text-[10px] text-muted-foreground pt-2">
          ApexKit Build Information
        </div>
      </div>
    </Dialog>
  );
};
