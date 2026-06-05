import React, { useEffect, useRef } from 'react';
import { X, AlertTriangle } from 'lucide-react';
import { Button } from './Elements';
import { createPortal } from 'react-dom';

interface DialogProps {
  isOpen: boolean;
  onClose: () => void;
  children?: React.ReactNode;
  title?: string;
  description?: string;
  size?: 'sm' | 'md' | 'lg' | 'xl' | 'full';
  zIndex?: number;
}

export const Dialog = ({
  isOpen,
  onClose,
  children,
  title,
  description,
  size = 'md',
  zIndex = 60,
}: DialogProps) => {
  const overlayRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (isOpen) {
      document.addEventListener('keydown', handleEscape);
      document.body.style.overflow = 'hidden';
    }
    return () => {
      document.removeEventListener('keydown', handleEscape);
      document.body.style.overflow = 'unset';
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const sizeClasses = {
    sm: 'sm:max-w-sm',
    md: 'sm:max-w-lg',
    lg: 'sm:max-w-2xl',
    xl: 'sm:max-w-4xl',
    full: 'sm:max-w-[95vw]',
  };

  return createPortal(
    <div
      className="fixed inset-0 flex items-end sm:items-center justify-center sm:p-4"
      style={{ zIndex }}
    >
      <div
        ref={overlayRef}
        className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in duration-200"
        onClick={onClose}
      />

      <div
        className={`relative z-50 w-full ${sizeClasses[size]} max-h-[95dvh] sm:max-h-[85vh] flex flex-col gap-4 border-t sm:border border-border bg-background p-6 shadow-lg animate-in slide-in-from-bottom sm:slide-in-from-bottom-0 sm:zoom-in-95 duration-300 rounded-t-xl sm:rounded-lg`}
      >
        <div className="flex flex-col space-y-1.5 text-left mb-2 shrink-0 pr-8">
          {title && <h2 className="text-lg font-semibold leading-none tracking-tight">{title}</h2>}
          {description && <p className="text-sm text-muted-foreground">{description}</p>}
        </div>

        <div className="overflow-y-auto -mx-6 px-6 flex-1 custom-scrollbar">{children}</div>

        <button
          onClick={onClose}
          className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground bg-secondary/10 p-1"
        >
          <X className="h-4 w-4" />
          <span className="sr-only">Close</span>
        </button>
      </div>
    </div>,
    document.body
  );
};

interface ConfirmationModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  description: string;
  confirmText?: string;
  cancelText?: string;
  variant?: 'destructive' | 'primary';
  isLoading?: boolean;
}

export const ConfirmationModal = ({
  isOpen,
  onClose,
  onConfirm,
  title,
  description,
  confirmText = 'Confirm',
  cancelText = 'Cancel',
  variant = 'primary',
  isLoading,
}: ConfirmationModalProps) => {
  return (
    <Dialog isOpen={isOpen} onClose={onClose} size="sm">
      <div className="flex flex-col gap-4">
        <div className="flex items-start gap-4">
          {variant === 'destructive' && (
            <div className="rounded-full bg-destructive/10 p-3 shrink-0">
              <AlertTriangle className="h-6 w-6 text-destructive" />
            </div>
          )}
          <div className="space-y-2">
            <h3 className="font-semibold">{title}</h3>
            <p className="text-sm text-muted-foreground">{description}</p>
          </div>
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <Button
            variant="outline"
            onClick={onClose}
            disabled={isLoading}
            className="flex-1 sm:flex-none"
          >
            {cancelText}
          </Button>
          <Button
            variant={variant}
            onClick={onConfirm}
            isLoading={isLoading}
            className="flex-1 sm:flex-none"
          >
            {confirmText}
          </Button>
        </div>
      </div>
    </Dialog>
  );
};
