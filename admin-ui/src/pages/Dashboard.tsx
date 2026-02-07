import React, { useEffect, useState } from 'react';
import { RefreshCw, Activity, HardDrive, Database, FileText, Loader2 } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Button, Badge } from '../components/ui/Elements';
import { LineChart } from '../components/charts/LineChart';
import { apiClient } from '../lib/apiClient'; // You need to add dashboard method here (see below)
import { useToast } from '../components/feedback/Toast';

export const Dashboard = () => {
  const [data, setData] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const { toast } = useToast();

  const fetchData = async () => {
    setLoading(true);
    try {
        // You need to add this method to your SDK or fetch manually
        const res = await apiClient.getAdminDashboardStats(); 
        setData(res);
    } catch (e) {
        toast('Failed to load dashboard data', 'error');
    } finally {
        setLoading(false);
    }
  };

  useEffect(() => { fetchData(); }, []);

  if (loading && !data) {
      return <div className="flex h-full items-center justify-center"><Loader2 className="animate-spin text-primary" /></div>;
  }

  const chartLines = [
    { dataKey: 'requests', color: 'hsl(var(--primary))' },
    { dataKey: 'errors', color: 'hsl(var(--destructive))' },
  ];

  const { stats, chart, recent_logs } = data || { stats: {}, chart: [], recent_logs: [] };

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <h2 className="text-3xl font-bold tracking-tight">Dashboard</h2>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={fetchData} disabled={loading}>
            <RefreshCw className={`mr-2 h-4 w-4 ${loading ? 'animate-spin' : ''}`} /> Refresh
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {[
          { label: "Total Requests", val: stats.total_requests?.toLocaleString() || "0", icon: Activity, trend: "All Time" },
          { label: "Database Size", val: `${stats.db_size_mb || 0} MB`, icon: HardDrive, trend: "Physical" },
          { label: "Collections", val: stats.collections_count || 0, icon: Database, trend: "Active" },
          { label: "Total Records", val: stats.total_records?.toLocaleString() || "0", icon: FileText, trend: "Across DB" },
        ].map((stat, i) => (
          <Card key={i}>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">{stat.label}</CardTitle>
              <stat.icon className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{stat.val}</div>
              <p className="text-xs text-muted-foreground mt-1">{stat.trend}</p>
            </CardContent>
          </Card>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-7">
        <div className="lg:col-span-4">
            <LineChart data={chart} lines={chartLines} title="Request Volume (7 Days)" />
        </div>
        <Card className="lg:col-span-3">
          <CardHeader>
            <CardTitle>Recent Logs</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {recent_logs.length === 0 ? <p className="text-sm text-muted-foreground">No recent activity.</p> : recent_logs.map((log: any) => (
                <div key={log.id} className="flex items-center justify-between border-b border-border pb-2 last:border-0 last:pb-0">
                  <div className="space-y-1 overflow-hidden">
                    <p className="text-sm font-medium leading-none truncate" title={log.message}>{log.message}</p>
                    <p className="text-xs text-muted-foreground">{log.source} • {new Date(log.timestamp).toLocaleTimeString()}</p>
                  </div>
                  <Badge variant={log.level === 'error' ? 'destructive' : log.level === 'warning' ? 'warning' : 'secondary'}>
                    {log.level}
                  </Badge>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
};