import React from 'react';
import { AlertCircle, CheckCircle, Info, TriangleAlert } from 'lucide-react';

interface AlertProps {
  variant?: 'default' | 'destructive' | 'success' | 'warning';
  children?: React.ReactNode;
  className?: string;
}

const variants = {
  default: {
    icon: Info,
    className: 'bg-secondary/20 border-border text-foreground',
  },
  destructive: {
    icon: AlertCircle,
    className: 'bg-destructive/10 border-destructive/20 text-destructive',
  },
  success: {
    icon: CheckCircle,
    className: 'bg-emerald-500/10 border-emerald-500/20 text-emerald-500',
  },
  warning: {
    icon: TriangleAlert,
    className: 'bg-amber-500/10 border-amber-500/20 text-amber-500',
  },
};

export const Alert = ({ variant = 'default', children, className }: AlertProps) => {
  const config = variants[variant];
  const Icon = config.icon;

  return (
    <div
      className={`flex items-start gap-3 rounded-md border p-4 text-sm ${config.className} ${className}`}
      role="alert"
    >
      <Icon className="h-5 w-5 shrink-0" />
      <div className="flex-1">{children}</div>
    </div>
  );
};