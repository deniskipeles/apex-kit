
import React, { useLayoutEffect, useState } from 'react';
import { createPortal } from 'react-dom';

interface OverlayProps {
  isOpen: boolean;
  onClose: () => void;
  anchorRef?: React.RefObject<HTMLElement>;
  children?: React.ReactNode;
  width?: string | number;
  className?: string;
  align?: 'start' | 'end' | 'center';
  zIndex?: number;
}

export const Overlay = ({ 
  isOpen, 
  onClose, 
  anchorRef, 
  children, 
  width = 'auto', 
  className = '',
  align = 'start',
  zIndex = 60
}: OverlayProps) => {
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (isOpen && anchorRef && anchorRef.current) {
      const updatePosition = () => {
        const rect = anchorRef.current!.getBoundingClientRect();
        const scrollX = window.scrollX;
        const scrollY = window.scrollY;

        let left = rect.left + scrollX;
        
        if (align === 'end') {
          left = rect.right + scrollX; 
        } else if (align === 'center') {
          left = rect.left + scrollX + (rect.width / 2);
        }

        setPosition({
          top: rect.bottom + scrollY + 6, 
          left: left
        });
      };
      
      updatePosition();
      window.addEventListener('resize', updatePosition);
      window.addEventListener('scroll', updatePosition, true);
      
      return () => {
        window.removeEventListener('resize', updatePosition);
        window.removeEventListener('scroll', updatePosition, true);
      };
    }
  }, [isOpen, anchorRef, align]);

  if (!isOpen) return null;

  return createPortal(
    <>
      <div 
        className="fixed inset-0 bg-transparent" 
        style={{ zIndex: zIndex }}
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }} 
      />
      
      <div 
        className={`fixed animate-in fade-in zoom-in-95 duration-100 ${className}`}
        style={{ 
          zIndex: zIndex + 1,
          top: position?.top ?? 0, 
          left: position?.left ?? 0,
          width: width,
          visibility: anchorRef && !position ? 'hidden' : 'visible'
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </>,
    document.body
  );
};
