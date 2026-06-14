import React, { useEffect, useState } from 'react';
import {
  RefreshCw,
  Activity,
  HardDrive,
  Database,
  FileText,
  Loader2,
  Server,
  TrendingUp,
  AlertCircle,
  Terminal,
  BrainCircuit,
  Search
} from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Button, Badge } from '../components/ui/Elements';
import { LineChart } from '../components/charts/LineChart';
import { apiClient } from '../lib/apiClient';
import { useToast } from '../components/feedback/Toast';

export const Dashboard = () => {
  const [data, setData] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const { toast } = useToast();

  const fetchData = async () => {
    setLoading(true);
    try {
      const res = await apiClient.getAdminDashboardStats();
      setData(res);
    } catch (e: any) {
      toast(e.message || 'Failed to load dashboard data', 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, []);

  if (loading && !data) {
    return (
      <div className="flex h-[80vh] items-center justify-center flex-col gap-4">
        <Loader2 className="animate-spin text-primary h-10 w-10" />
        <p className="text-muted-foreground animate-pulse text-sm font-medium">
          Loading system metrics...
        </p>
      </div>
    );
  }

  const chartLines = [
    { dataKey: 'requests', color: 'hsl(var(--primary))' },
    { dataKey: 'errors', color: 'hsl(var(--destructive))' },
  ];

  const { stats, chart, recent_logs } = data || { stats: {}, chart: [], recent_logs: [] };

  const getBadgeVariant = (level: string) => {
    const lvl = level.toLowerCase();
    if (lvl === 'error' || lvl === 'critical') return 'destructive';
    if (lvl === 'warning' || lvl === 'warn') return 'warning';
    if (lvl === 'success') return 'success';
    return 'secondary';
  };

  return (
    <div className="space-y-6 pb-20 animate-in fade-in slide-in-from-bottom-2 duration-500 max-w-7xl mx-auto">
      {/* Header Section */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 border-b border-border/50 pb-5">
        <div>
          <h2 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <Server className="h-8 w-8 text-primary" /> System Overview
          </h2>
          <p className="text-muted-foreground mt-1">
            High-level view of your application infrastructure and usage.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            onClick={fetchData}
            disabled={loading}
            className="shadow-sm bg-background"
          >
            <RefreshCw className={`mr-2 h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
            {loading ? 'Refreshing...' : 'Refresh Metrics'}
          </Button>
        </div>
      </div>

      {/* Top Metric Cards */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
        {[
          {
            label: 'Total Requests',
            val: stats.total_requests?.toLocaleString() || '0',
            icon: Activity,
            trend: 'Overall API Traffic',
            color: 'text-blue-500',
            bg: 'bg-blue-500/10',
          },
          {
            label: 'Database Size',
            val: `${stats.db_size_mb?.toFixed(2) || 0} MB`,
            icon: HardDrive,
            trend: 'Physical DB Usage',
            color: 'text-purple-500',
            bg: 'bg-purple-500/10',
          },
          {
            label: 'Collections',
            val: stats.collections_count?.toLocaleString() || '0',
            icon: Database,
            trend: 'Active Schema Tables',
            color: 'text-emerald-500',
            bg: 'bg-emerald-500/10',
          },
          {
            label: 'Total Records',
            val: stats.total_records?.toLocaleString() || '0',
            icon: FileText,
            trend: 'Combined Row Count',
            color: 'text-amber-500',
            bg: 'bg-amber-500/10',
          },
          {
            label: 'Total Vectors',
            val: stats.total_vectors?.toLocaleString() || '0',
            icon: BrainCircuit,
            trend: 'AI Embeddings',
            color: 'text-indigo-500',
            bg: 'bg-indigo-500/10',
          },
          {
            label: 'Indexes Size',
            val: `${stats.indexes_size_mb?.toFixed(2) || 0} MB`,
            icon: Search,
            trend: 'Tantivy Search Size',
            color: 'text-pink-500',
            bg: 'bg-pink-500/10',
          },
        ].map((stat, i) => (
          <Card
            key={i}
            className="hover:border-primary/40 transition-colors group overflow-hidden relative"
          >
            <div
              className={`absolute top-0 right-0 p-4 ${stat.bg} rounded-bl-3xl opacity-50 group-hover:opacity-100 transition-opacity`}
            >
              <stat.icon className={`h-5 w-5 ${stat.color}`} />
            </div>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-bold text-muted-foreground uppercase tracking-wider truncate pr-6">
                {stat.label}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-extrabold text-foreground tracking-tight truncate">
                {stat.val}
              </div>
              <div className="flex items-center gap-1.5 mt-2">
                <TrendingUp className="h-3 w-3 text-muted-foreground shrink-0" />
                <p className="text-[10px] text-muted-foreground uppercase font-semibold truncate">
                  {stat.trend}
                </p>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Main Content Area (Chart + Logs) */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-7">
        {/* Chart Column */}
        <div className="lg:col-span-4 h-full">
          <div className="h-full flex flex-col">
            <LineChart
              data={chart}
              lines={chartLines}
              title="API Traffic & Errors (7 Days)"
              isLoading={loading}
            />
          </div>
        </div>

        {/* Recent Logs Column */}
        <Card className="lg:col-span-3 flex flex-col h-[400px] lg:h-auto">
          <CardHeader className="border-b border-border/50 pb-4 bg-secondary/5">
            <CardTitle className="flex items-center justify-between text-base">
              <span className="flex items-center gap-2">
                <AlertCircle className="h-4 w-4 text-primary" /> Live Log Feed
              </span>
              <Badge variant="outline" className="text-[10px] font-mono">
                Last 10 Events
              </Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="p-0 flex-1 overflow-hidden flex flex-col">
            <div className="overflow-y-auto flex-1 custom-scrollbar">
              {recent_logs.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-full text-muted-foreground gap-3 opacity-60">
                  <Terminal className="h-10 w-10" />
                  <p className="text-sm font-medium">System is quiet.</p>
                </div>
              ) : (
                <div className="divide-y divide-border/50">
                  {recent_logs.map((log: any) => (
                    <div
                      key={log.id}
                      className="p-4 hover:bg-secondary/10 transition-colors group flex gap-3"
                    >
                      <div className="mt-0.5 shrink-0">
                        <Badge
                          variant={getBadgeVariant(log.level)}
                          className="uppercase text-[9px] font-bold px-1.5 py-0.5"
                        >
                          {log.level}
                        </Badge>
                      </div>
                      <div className="min-w-0 flex-1">
                        <p
                          className="text-sm font-medium leading-relaxed text-foreground/90 break-words line-clamp-2"
                          title={log.message}
                        >
                          {log.message}
                        </p>
                        <div className="flex items-center justify-between mt-2">
                          <span className="text-[10px] font-mono text-primary bg-primary/5 border border-primary/10 px-1.5 py-0.5 rounded truncate max-w-[150px]">
                            {log.source}
                          </span>
                          <span className="text-[10px] text-muted-foreground font-mono">
                            {new Date(log.timestamp).toLocaleTimeString([], {
                              hour: '2-digit',
                              minute: '2-digit',
                              second: '2-digit',
                            })}
                          </span>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
};