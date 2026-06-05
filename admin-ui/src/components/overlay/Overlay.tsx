import React, { useLayoutEffect, useState, useRef } from 'react';
import { createPortal } from 'react-dom';

interface OverlayProps {
  isOpen: boolean;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLElement>;
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
  zIndex = 100, // Default high z-index
}: OverlayProps) => {
  const [style, setStyle] = useState<React.CSSProperties>({ opacity: 0 });
  const overlayRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    if (isOpen && anchorRef.current && overlayRef.current) {
      const updatePosition = () => {
        const triggerRect = anchorRef.current!.getBoundingClientRect();
        const overlayRect = overlayRef.current!.getBoundingClientRect();
        const viewportHeight = window.innerHeight;
        const viewportWidth = window.innerWidth;

        let top = 0;
        let left = 0;

        // 1. Vertical Positioning (Flip Logic)
        const spaceBelow = viewportHeight - triggerRect.bottom;
        const spaceAbove = triggerRect.top;
        const overlayHeight = overlayRect.height;

        // If not enough space below, and there is space above, flip it
        if (spaceBelow < overlayHeight && spaceAbove > overlayHeight) {
          // Position ABOVE
          top = triggerRect.top - overlayHeight - 8; // 8px buffer
        } else {
          // Position BELOW (Default)
          top = triggerRect.bottom + 8;
        }

        // 2. Horizontal Positioning
        if (align === 'end') {
          left = triggerRect.right - overlayRect.width;
        } else if (align === 'center') {
          left = triggerRect.left + triggerRect.width / 2 - overlayRect.width / 2;
        } else {
          left = triggerRect.left;
        }

        // 3. Prevent Horizontal Overflow (Keep on screen)
        if (left + overlayRect.width > viewportWidth) {
          left = viewportWidth - overlayRect.width - 10;
        }
        if (left < 0) {
          left = 10;
        }

        setStyle({
          position: 'fixed', // Fixed ensures it stays relative to viewport, not document
          top: `${top}px`,
          left: `${left}px`,
          width: width,
          opacity: 1, // Make visible only after calculation
          zIndex: zIndex,
        });
      };

      updatePosition();

      // Update on scroll or resize
      window.addEventListener('resize', updatePosition);
      window.addEventListener('scroll', updatePosition, true);

      return () => {
        window.removeEventListener('resize', updatePosition);
        window.removeEventListener('scroll', updatePosition, true);
      };
    }
  }, [isOpen, anchorRef, align, width, zIndex]);

  if (!isOpen) return null;

  return createPortal(
    <>
      {/* Backdrop to handle click-outside */}
      <div
        className="fixed inset-0 bg-transparent"
        style={{ zIndex: zIndex - 1 }}
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
      />

      {/* The Overlay Content */}
      <div
        ref={overlayRef}
        className={`fixed animate-in fade-in zoom-in-95 duration-100 ${className}`}
        style={style}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </>,
    document.body
  );
};
