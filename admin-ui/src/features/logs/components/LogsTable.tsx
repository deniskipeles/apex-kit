import React from 'react';
import { Terminal } from 'lucide-react';
import { SystemLog } from '../../../types';
import { Badge } from '../../../components/ui/Elements';

interface LogsTableProps {
  logs: SystemLog[];
  isLoading?: boolean;
}

export const LogsTable = ({ logs, isLoading }: LogsTableProps) => {
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

  return (
    <div className="rounded-md border border-border overflow-hidden bg-card shadow-sm">
      {/* Desktop Header */}
      <div className="hidden bg-muted/50 px-4 py-3 text-xs font-semibold text-muted-foreground md:grid grid-cols-12 gap-4 border-b border-border">
        <div className="col-span-2">Timestamp</div>
        <div className="col-span-1">Level</div>
        <div className="col-span-3">Source / Target</div>
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
          logs.map((log) => (
            <div key={log.id} className="font-mono text-xs hover:bg-muted/10 transition-colors">
              {/* Mobile View Card */}
              <div className="flex flex-col gap-2 p-3.5 md:hidden">
                <div className="flex items-center justify-between">
                  <Badge
                    variant={getBadgeVariant(log.level)}
                    className="uppercase text-[9px] font-bold px-1.5 py-0.5"
                  >
                    {log.level}
                  </Badge>
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
              <div className="hidden grid-cols-12 gap-4 px-4 py-3 items-center md:grid">
                <div className="col-span-2 text-muted-foreground">{formatDate(log.timestamp)}</div>
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
            </div>
          ))
        )}
      </div>
    </div>
  );
};
