import React, { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { X, Edit, Trash2 } from 'lucide-react';
import { Button } from '../form/FormPrimitives';

interface PreviewPanelProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children?: React.ReactNode;
  actions?: React.ReactNode;
}

export const PreviewPanel = ({ isOpen, onClose, title, children, actions }: PreviewPanelProps) => {
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    if (isOpen) {
      document.addEventListener('keydown', handleEsc);
      document.body.style.overflow = 'hidden';
    }
    return () => {
      document.removeEventListener('keydown', handleEsc);
      document.body.style.overflow = 'unset';
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return createPortal(
    <div className="fixed inset-0 z-[50] flex justify-end isolate">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-[1px] animate-in fade-in"
        onClick={onClose}
      />
      <div className="relative w-full h-full md:max-w-lg bg-background border-l border-border shadow-2xl animate-in slide-in-from-right duration-300 flex flex-col">
        <div className="flex items-center justify-between p-4 sm:p-6 border-b bg-secondary/5 shrink-0 safe-top">
          <h2 className="font-bold text-lg truncate pr-4">{title}</h2>
          <Button size="icon" variant="ghost" onClick={onClose}>
            <X className="h-5 w-5" />
          </Button>
        </div>
        <div className="flex-1 overflow-y-auto p-4 sm:p-6">{children}</div>
        {actions && (
          <div className="p-4 border-t border-border bg-background flex gap-3 shrink-0 safe-bottom">
            {actions}
          </div>
        )}
      </div>
    </div>,
    document.body
  );
};
