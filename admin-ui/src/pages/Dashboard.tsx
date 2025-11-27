
import React from 'react';
import { RefreshCw, Activity, HardDrive, Database, FileText } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, Button, Badge } from '../components/ui/Elements';
import { LineChart } from '../components/charts/LineChart';
import { CHART_DATA, MOCK_LOGS } from '../constants';

export const Dashboard = () => {
  const chartLines = [
    { dataKey: 'requests', color: 'hsl(var(--primary))' },
    { dataKey: 'errors', color: 'hsl(var(--destructive))' },
  ];
  
  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <h2 className="text-3xl font-bold tracking-tight">Dashboard</h2>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm">
            <RefreshCw className="mr-2 h-4 w-4" /> Refresh
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {[
          { label: "Total Requests", val: "1.2M", icon: Activity, trend: "+12%" },
          { label: "Database Size", val: "432 MB", icon: HardDrive, trend: "+2%" },
          { label: "Collections", val: "12", icon: Database, trend: "0%" },
          { label: "Total Records", val: "43,291", icon: FileText, trend: "+540" },
        ].map((stat, i) => (
          <Card key={i}>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">{stat.label}</CardTitle>
              <stat.icon className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{stat.val}</div>
              <p className="text-xs text-emerald-500 mt-1">{stat.trend} from last month</p>
            </CardContent>
          </Card>
        ))}
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-7">
        <div className="lg:col-span-4">
            <LineChart data={CHART_DATA} lines={chartLines} title="Request Volume" />
        </div>
        <Card className="lg:col-span-3">
          <CardHeader>
            <CardTitle>System Logs</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {MOCK_LOGS.slice(0, 5).map((log) => (
                <div key={log.id} className="flex items-center justify-between border-b border-border pb-2 last:border-0 last:pb-0">
                  <div className="space-y-1">
                    <p className="text-sm font-medium leading-none">{log.message}</p>
                    <p className="text-xs text-muted-foreground">{log.source} • {new Date(log.timestamp).toLocaleTimeString()}</p>
                  </div>
                  <Badge variant={log.level === 'error' ? 'destructive' : log.level === 'warning' ? 'secondary' : 'default'}>
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
