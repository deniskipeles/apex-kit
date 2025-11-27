
import React from 'react';

export const AuthLayout = ({ children }: { children: React.ReactNode }) => {
  return (
    <div className="flex min-h-screen items-center justify-center bg-background text-foreground p-4">
      <div className="w-full max-w-md animate-in fade-in zoom-in-95 duration-300">
        {children}
      </div>
    </div>
  );
};
