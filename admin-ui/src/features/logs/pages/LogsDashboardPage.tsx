import React, { useEffect, useState } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { Button } from '../../../components/ui/Elements';
import { LogsChart } from '../components/LogsChart';
import { LogsTable } from '../components/LogsTable';
import { logsService } from '../services/logsService';
import { SystemLog } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { CHART_DATA } from '../../../constants'; // Fallback mock data for chart

export const LogsDashboardPage = () => {
  const [logs, setLogs] = useState<SystemLog[]>([]);
  const [stats, setStats] = useState<any[]>(CHART_DATA); // Use constant for chart for now
  const [loading, setLoading] = useState(true);
  const { toast } = useToast();

  const fetchData = async () => {
    setLoading(true);
    try {
        // Fetch real logs from backend
        const logsData = await logsService.list();
        setLogs(logsData);
        
        // Calculate simple stats from real logs for the chart
        // This is a basic client-side aggregation
        const statsMap: Record<string, number> = {};
        logsData.forEach(log => {
            const day = new Date(log.timestamp).toLocaleDateString('en-US', { weekday: 'short' });
            statsMap[day] = (statsMap[day] || 0) + 1;
        });
        
        // If we have real log data, map it to chart format, otherwise keep mock
        if (logsData.length > 0) {
            const chartData = Object.keys(statsMap).map(name => ({
                name,
                requests: statsMap[name], // Mapping all logs to "requests" for visual
                errors: logsData.filter(l => l.level === 'error' && new Date(l.timestamp).toLocaleDateString('en-US', { weekday: 'short' }) === name).length
            }));
            setStats(chartData);
        }

    } catch (e) {
        console.error(e);
        toast('Failed to load logs', 'error');
    } finally {
        setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, []);

  return (
    <div className="space-y-6">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
            <div>
                <h2 className="text-3xl font-bold tracking-tight">System Logs</h2>
                <p className="text-muted-foreground">Monitor system health and application activity.</p>
            </div>
            <div className="flex gap-2">
                <Button variant="outline" onClick={fetchData} disabled={loading}>
                    <RefreshCw className={`mr-2 h-4 w-4 ${loading ? 'animate-spin' : ''}`} /> Refresh
                </Button>
                {/* Clear logs not implemented in backend yet */}
                <Button variant="destructive" disabled> 
                    <Trash2 className="mr-2 h-4 w-4" /> Clear Logs
                </Button>
            </div>
        </div>

        <LogsChart data={stats} isLoading={loading} />
        
        <div className="space-y-2">
            <h3 className="text-lg font-semibold">Recent Activity</h3>
            <LogsTable logs={logs} isLoading={loading} />
        </div>
    </div>
  );
};