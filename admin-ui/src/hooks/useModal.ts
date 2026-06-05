import { useState, useCallback } from 'react';

export function useModal<T = any>() {
  const [isOpen, setIsOpen] = useState(false);
  const [data, setData] = useState<T | null>(null);

  const open = useCallback((modalData?: T) => {
    if (modalData) setData(modalData);
    setIsOpen(true);
  }, []);

  const close = useCallback(() => {
    setIsOpen(false);
    setTimeout(() => setData(null), 200); // Clear data after animation
  }, []);

  const toggle = useCallback(() => setIsOpen((prev) => !prev), []);

  return {
    isOpen,
    data,
    open,
    close,
    toggle,
  };
}
