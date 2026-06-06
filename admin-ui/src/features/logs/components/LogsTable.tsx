import React, { useState } from 'react';
import { Terminal, ChevronDown, ChevronRight, Globe, ShieldAlert, Cpu } from 'lucide-react';
import { SystemLog } from '../../../types';
import { Badge } from '../../../components/ui/Elements';

interface LogsTableProps {
  logs: SystemLog[];
  isLoading?: boolean;
}

export const LogsTable = ({ logs, isLoading }: LogsTableProps) => {
  const [expandedLogId, setExpandedLogId] = useState<string | null>(null);

  const getBadgeVariant = (level: string) => {
    const lvl = level.toLowerCase();
    if (lvl === 'error' || lvl === 'critical' || lvl === 'fail') return 'destructive';
    if (lvl === 'warn' || lvl === 'warning') return 'warning';
    if (lvl === 'success' || lvl === 'info') return 'success';
    return 'secondary';
  };

  const formatDate = (date: string) =>
    new Date(date).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });

  const toggleExpand = (id: string) => {
    setExpandedLogId(expandedLogId === id ? null : id);
  };

  return (
    <div className="rounded-md border border-border overflow-hidden bg-card shadow-sm">
      {/* Desktop Header */}
      <div className="hidden bg-muted/50 px-4 py-3 text-xs font-semibold text-muted-foreground md:grid grid-cols-12 gap-4 border-b border-border">
        <div className="col-span-2">Timestamp</div>
        <div className="col-span-1">Level</div>
        <div className="col-span-3">Source / Module</div>
        <div className="col-span-6">Log Message</div>
      </div>
      <div className="divide-y divide-border/50">
        {isLoading ? (
          Array.from({ length: 10 }).map((_, i) => (
            <div
              key={i}
              className="px-4 py-3.5 animate-pulse bg-background/50 grid grid-cols-12 gap-4"
            >
              <div className="h-4 col-span-2 bg-muted rounded" />
              <div className="h-4 col-span-1 bg-muted rounded" />
              <div className="h-4 col-span-3 bg-muted rounded" />
              <div className="h-4 col-span-6 bg-muted rounded" />
            </div>
          ))
        ) : logs.length === 0 ? (
          <div className="p-12 text-center text-muted-foreground flex flex-col items-center justify-center gap-2">
            <Terminal className="h-8 w-8 opacity-30" />
            <p className="font-medium text-sm text-foreground">No log entries found</p>
            <p className="text-xs opacity-70">
              Adjust your filters or queries to search other time ranges.
            </p>
          </div>
        ) : (
          logs.map((log) => {
            const isExpanded = expandedLogId === log.id;
            const hasMeta = !!log.meta && Object.keys(log.meta).length > 0;

            return (
              <div key={log.id} className="font-mono text-xs hover:bg-muted/5 transition-colors">
                {/* Mobile View Card */}
                <div
                  className="flex flex-col gap-2 p-3.5 md:hidden"
                  onClick={() => hasMeta && toggleExpand(log.id)}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Badge
                        variant={getBadgeVariant(log.level)}
                        className="uppercase text-[9px] font-bold px-1.5 py-0.5"
                      >
                        {log.level}
                      </Badge>
                      {hasMeta && (
                        <span className="text-[10px] text-primary flex items-center gap-0.5">
                          {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                          Metadata
                        </span>
                      )}
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      {formatDate(log.timestamp)}
                    </div>
                  </div>
                  <p className="text-foreground/90 font-medium break-all leading-relaxed">
                    {log.message}
                  </p>
                  <div className="text-[10px]">
                    <span className="text-muted-foreground">Source: </span>
                    <span className="font-semibold text-primary">{log.source}</span>
                  </div>
                </div>

                {/* Desktop View Row */}
                <div
                  className={`hidden grid-cols-12 gap-4 px-4 py-3 items-center md:grid ${hasMeta ? 'cursor-pointer' : ''}`}
                  onClick={() => hasMeta && toggleExpand(log.id)}
                >
                  <div className="col-span-2 text-muted-foreground flex items-center gap-1.5">
                    {hasMeta && (
                      <span className="text-muted-foreground/60">
                        {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      </span>
                    )}
                    {formatDate(log.timestamp)}
                  </div>
                  <div className="col-span-1">
                    <Badge
                      variant={getBadgeVariant(log.level)}
                      className="uppercase text-[9px] font-bold px-1.5 py-0.5"
                    >
                      {log.level}
                    </Badge>
                  </div>
                  <div
                    className="col-span-3 font-semibold text-primary/90 truncate"
                    title={log.source}
                  >
                    {log.source}
                  </div>
                  <div
                    className="col-span-6 truncate text-foreground/90 select-all"
                    title={log.message}
                  >
                    {log.message}
                  </div>
                </div>

                {/* [NEW] Expanded Audit Metadata Area */}
                {isExpanded && log.meta && (
                  <div className="px-6 py-4 bg-secondary/10 border-t border-b border-border/30 grid grid-cols-1 md:grid-cols-2 gap-4 animate-in slide-in-from-top-1 duration-200">
                    <div className="space-y-2">
                      <h4 className="font-bold text-xs uppercase tracking-wider text-muted-foreground flex items-center gap-1.5">
                        <Globe className="h-3.5 w-3.5" /> Connection context
                      </h4>
                      <div className="space-y-1 text-[11px] text-foreground/80">
                        <div>
                          <span className="text-muted-foreground">Client IP:</span>{' '}
                          <span className="font-bold">{log.meta.ip || 'Local / Unknown'}</span>
                        </div>
                        <div className="truncate" title={log.meta.user_agent}>
                          <span className="text-muted-foreground">User Agent:</span>{' '}
                          {log.meta.user_agent || '-'}
                        </div>
                        <div className="truncate" title={log.meta.referer}>
                          <span className="text-muted-foreground">Referer:</span>{' '}
                          {log.meta.referer || '-'}
                        </div>
                      </div>
                    </div>

                    <div className="space-y-2">
                      <h4 className="font-bold text-xs uppercase tracking-wider text-muted-foreground flex items-center gap-1.5">
                        <Cpu className="h-3.5 w-3.5" /> Payload Metadata
                      </h4>
                      <div className="bg-[#0f0f11] rounded p-2.5 border font-mono text-[10px] text-blue-100 max-h-32 overflow-y-auto">
                        <pre>{JSON.stringify(log.meta, null, 2)}</pre>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};
