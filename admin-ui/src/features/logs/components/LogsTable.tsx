import React, { useState } from 'react';
import { Search, Filter, Terminal } from 'lucide-react';
import { SystemLog } from '../../../types';
import { Badge, Input, Select, Button } from '../../../components/ui/Elements';

interface LogsTableProps {
  logs: SystemLog[];
  isLoading?: boolean;
}

export const LogsTable = ({ logs, isLoading }: LogsTableProps) => {
  const [search, setSearch] = useState('');
  const [levelFilter, setLevelFilter] = useState('');

  const filteredLogs = logs.filter((log) => {
    const matchesSearch =
      log.message.toLowerCase().includes(search.toLowerCase()) ||
      log.source.toLowerCase().includes(search.toLowerCase());
    const matchesLevel = levelFilter ? log.level === levelFilter : true;
    return matchesSearch && matchesLevel;
  });

  const getBadgeVariant = (level: string) => {
    switch (level) {
      case 'error':
        return 'destructive';
      case 'warning':
        return 'warning';
      case 'success':
        return 'success';
      default:
        return 'secondary';
    }
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
    <div className="space-y-4">
      <div className="flex flex-col sm:flex-row gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search logs..."
            className="pl-9"
            value={search}
            onChange={(e: any) => setSearch(e.target.value)}
          />
        </div>
        <Select
          className="w-full sm:w-[180px]"
          value={levelFilter}
          onChange={(e: any) => setLevelFilter(e.target.value)}
        >
          <option value="">All Levels</option>
          <option value="info">Info</option>
          <option value="success">Success</option>
          <option value="warning">Warning</option>
          <option value="error">Error</option>
        </Select>
      </div>

      <div className="rounded-md border border-border overflow-hidden bg-card">
        {/* Desktop Header */}
        <div className="hidden bg-muted/50 px-4 py-3 text-xs font-medium text-muted-foreground md:grid grid-cols-12 gap-4">
          <div className="col-span-2">Timestamp</div>
          <div className="col-span-1">Level</div>
          <div className="col-span-2">Source</div>
          <div className="col-span-7">Message</div>
        </div>
        <div className="md:divide-y md:divide-border">
          {isLoading ? (
            <div className="p-8 text-center text-muted-foreground">Loading logs...</div>
          ) : filteredLogs.length === 0 ? (
            <div className="p-8 text-center text-muted-foreground flex flex-col items-center gap-2">
              <Terminal className="h-8 w-8 opacity-50" />
              <p>No logs found matching criteria</p>
            </div>
          ) : (
            filteredLogs.map((log) => (
              <div key={log.id} className="font-mono text-sm border-b border-border md:border-none">
                {/* Mobile View Card */}
                <div className="flex flex-col gap-2 p-3 md:hidden">
                  <div className="flex items-center justify-between">
                    <Badge variant={getBadgeVariant(log.level)} className="uppercase text-[10px]">
                      {log.level}
                    </Badge>
                    <div className="text-xs text-muted-foreground">{formatDate(log.timestamp)}</div>
                  </div>
                  <p className="truncate text-foreground/90" title={log.message}>
                    {log.message}
                  </p>
                  <div className="text-xs">
                    <span className="text-muted-foreground">Source: </span>
                    <span className="font-semibold text-primary/80">{log.source}</span>
                  </div>
                </div>

                {/* Desktop View Row */}
                <div className="hidden grid-cols-12 gap-4 px-4 py-3 items-center hover:bg-muted/30 transition-colors md:grid">
                  <div className="col-span-2 text-xs text-muted-foreground">
                    {formatDate(log.timestamp)}
                  </div>
                  <div className="col-span-1">
                    <Badge variant={getBadgeVariant(log.level)} className="uppercase text-[10px]">
                      {log.level}
                    </Badge>
                  </div>
                  <div className="col-span-2 text-xs font-semibold text-primary/80">
                    {log.source}
                  </div>
                  <div className="col-span-7 truncate" title={log.message}>
                    {log.message}
                  </div>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
