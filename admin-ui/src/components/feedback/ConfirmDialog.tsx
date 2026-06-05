import React from 'react';
import { createPortal } from 'react-dom';
import { AlertTriangle } from 'lucide-react';
import { Button } from '../form/FormPrimitives';

interface ConfirmDialogProps {
  isOpen: boolean;
  title: string;
  description: string;
  onConfirm: () => void;
  onCancel: () => void;
  variant?: 'destructive' | 'primary';
  confirmText?: string;
  isLoading?: boolean;
}

export const ConfirmDialog = ({
  isOpen,
  title,
  description,
  onConfirm,
  onCancel,
  variant = 'destructive',
  confirmText = 'Confirm',
  isLoading,
}: ConfirmDialogProps) => {
  if (!isOpen) return null;

  return createPortal(
    <div className="fixed inset-0 z-[100] flex items-center justify-center p-4 isolate">
      <div
        className="absolute inset-0 bg-black/50 backdrop-blur-sm animate-in fade-in"
        onClick={onCancel}
      />
      <div className="relative w-full max-w-md bg-background rounded-lg border border-border shadow-xl p-6 animate-in zoom-in-95 duration-200">
        <div className="flex gap-4">
          <div
            className={`h-10 w-10 rounded-full flex items-center justify-center shrink-0 ${variant === 'destructive' ? 'bg-destructive/10 text-destructive' : 'bg-primary/10 text-primary'}`}
          >
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="space-y-2">
            <h3 className="font-bold text-lg">{title}</h3>
            <p className="text-sm text-muted-foreground">{description}</p>
          </div>
        </div>
        <div className="flex justify-end gap-3 mt-6">
          <Button variant="outline" onClick={onCancel} disabled={isLoading}>
            Cancel
          </Button>
          <Button variant={variant} onClick={onConfirm} isLoading={isLoading}>
            {confirmText}
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
};
