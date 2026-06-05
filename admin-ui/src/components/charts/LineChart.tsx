import React from 'react';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts';
import { Card, CardHeader, CardTitle, CardContent } from '../ui/Elements';

interface ChartData {
  name: string;
  [key: string]: any;
}

interface LineChartProps {
  data: ChartData[];
  lines: { dataKey: string; color: string }[];
  title: string;
  isLoading?: boolean;
}

export const LineChart = ({ data, lines, title, isLoading }: LineChartProps) => {
  if (isLoading) {
    return (
      <Card className="h-[350px] animate-pulse bg-muted/20">
        <CardHeader>
          <div className="h-6 w-32 bg-muted rounded"></div>
        </CardHeader>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent className="pl-2">
        <div className="h-[300px] w-full">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data}>
              <defs>
                {lines.map((line) => (
                  <linearGradient
                    key={line.dataKey}
                    id={`color_${line.dataKey}`}
                    x1="0"
                    y1="0"
                    x2="0"
                    y2="1"
                  >
                    <stop offset="5%" stopColor={line.color} stopOpacity={0.3} />
                    <stop offset="95%" stopColor={line.color} stopOpacity={0} />
                  </linearGradient>
                ))}
              </defs>
              <XAxis
                dataKey="name"
                stroke="hsl(var(--muted-foreground))"
                fontSize={12}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                stroke="hsl(var(--muted-foreground))"
                fontSize={12}
                tickLine={false}
                axisLine={false}
              />
              <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
              <Tooltip
                contentStyle={{
                  backgroundColor: 'hsl(var(--popover))',
                  borderColor: 'hsl(var(--border))',
                  color: 'hsl(var(--popover-foreground))',
                  borderRadius: 'var(--radius)',
                }}
                itemStyle={{ color: 'hsl(var(--foreground))' }}
              />
              {lines.map((line) => (
                <Area
                  key={line.dataKey}
                  type="monotone"
                  dataKey={line.dataKey}
                  stroke={line.color}
                  strokeWidth={2}
                  fillOpacity={1}
                  fill={`url(#color_${line.dataKey})`}
                  name={line.dataKey.charAt(0).toUpperCase() + line.dataKey.slice(1)}
                />
              ))}
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </CardContent>
    </Card>
  );
};
