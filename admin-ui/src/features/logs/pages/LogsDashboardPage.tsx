import React, { useEffect, useState, useCallback } from 'react';
import { RefreshCw, Search, SlidersHorizontal, Terminal, Activity } from 'lucide-react';
import { Button, Input, Select, Label, Badge } from '../../../components/ui/Elements';
import { LogsChart } from '../components/LogsChart';
import { LogsTable } from '../components/LogsTable';
import { logsService } from '../services/logsService';
import { SystemLog } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { usePagination } from '../../../hooks/usePagination';
import { Pagination } from '../../../components/data/Pagination';

export const LogsDashboardPage = () => {
  const [logs, setLogs] = useState<SystemLog[]>([]);
  const [stats, setStats] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const { toast } = useToast();

  // Query Filter States
  const [searchQuery, setSearchQuery] = useState('');
  const [levelFilter, setLevelFilter] = useState('');
  const [sourceFilter, setSourceFilter] = useState('');

  // Pagination Hook
  const { page, perPage, setPage } = usePagination(1, 50);
  const [totalLogs, setTotalLogs] = useState(0);

  const fetchLogs = useCallback(async () => {
    setLoading(true);
    try {
      const res = await logsService.list(page, perPage, levelFilter, sourceFilter, searchQuery);
      setLogs(res.items);
      setTotalLogs(res.total);

      // Build dynamic aggregated stats for Charting based on active entries
      const statsMap: Record<string, { total: number; errors: number }> = {};
      const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
      days.forEach((d) => (statsMap[d] = { total: 0, errors: 0 }));

      res.items.forEach((log) => {
        const day = new Date(log.timestamp).toLocaleDateString('en-US', { weekday: 'short' });
        if (statsMap[day]) {
          statsMap[day].total += 1;
          const lvl = log.level.toLowerCase();
          if (
            lvl === 'error' ||
            lvl === 'critical' ||
            lvl === 'warn' ||
            lvl === 'warning' ||
            lvl === 'fail'
          ) {
            statsMap[day].errors += 1;
          }
        }
      });

      const chartData = days.map((day) => ({
        name: day,
        requests: statsMap[day].total,
        errors: statsMap[day].errors,
      }));
      setStats(chartData);
    } catch (e: any) {
      console.error(e);
      toast(e.message || 'Failed to load system logs', 'error');
    } finally {
      setLoading(false);
    }
  }, [page, perPage, levelFilter, sourceFilter, searchQuery, toast]);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  const handleSearchChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setSearchQuery(e.target.value);
    setPage(1); // Reset to page 1 on active typing search
  };

  return (
    <div className="space-y-6 max-w-7xl mx-auto pb-20 animate-in fade-in duration-300">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 border-b border-border/50 pb-5">
        <div>
          <h2 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <Terminal className="h-8 w-8 text-primary" /> System Logs
          </h2>
          <p className="text-muted-foreground">
            Monitor real-time system performance, routing traces, and script execution logs.
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" onClick={fetchLogs} disabled={loading} className="shadow-sm">
            <RefreshCw className={`mr-2 h-4 w-4 ${loading ? 'animate-spin' : ''}`} /> Refresh
          </Button>
        </div>
      </div>

      {/* Aggregate Graph visualization */}
      <LogsChart data={stats} isLoading={loading && logs.length === 0} />

      {/* Dynamic Filters */}
      <div className="space-y-3">
        <h3 className="text-lg font-bold flex items-center gap-2">
          <SlidersHorizontal className="h-4 w-4 text-primary" /> Active Logs Stream
        </h3>

        <div className="grid grid-cols-1 md:grid-cols-12 gap-3 bg-secondary/10 p-3 rounded-lg border border-border/50">
          <div className="md:col-span-6 relative">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search log messages or exceptions..."
              className="pl-9 bg-background"
              value={searchQuery}
              onChange={handleSearchChange}
            />
          </div>
          <div className="md:col-span-3">
            <Select
              value={levelFilter}
              onChange={(e: any) => {
                setLevelFilter(e.target.value);
                setPage(1);
              }}
              className="bg-background"
            >
              <option value="">All Severity Levels</option>
              <option value="info">Info</option>
              <option value="success">Success</option>
              <option value="warn">Warning</option>
              <option value="error">Error</option>
            </Select>
          </div>
          <div className="md:col-span-3">
            <Input
              placeholder="Filter by Source (e.g. ai, api)..."
              value={sourceFilter}
              onChange={(e: any) => {
                setSourceFilter(e.target.value);
                setPage(1);
              }}
              className="bg-background"
            />
          </div>
        </div>
      </div>

      {/* Main Grid View */}
      <div className="space-y-4">
        <div className="flex items-center justify-between text-xs text-muted-foreground px-1">
          <span className="flex items-center gap-1.5 font-medium">
            <Activity className="h-3.5 w-3.5" /> Total logs:{' '}
            <span className="font-bold text-foreground font-mono">{totalLogs}</span>
          </span>
          <span>Showing Page {page}</span>
        </div>

        <LogsTable logs={logs} isLoading={loading} />

        <div className="flex justify-end pt-2 bg-transparent">
          <Pagination
            page={page}
            totalPages={Math.ceil(totalLogs / perPage) || 1}
            onPageChange={setPage}
          />
        </div>
      </div>
    </div>
  );
};
