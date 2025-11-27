
import React, { useState } from 'react';
import { Loader2 } from 'lucide-react';
import { AuthProvider, useAuth } from '../features/auth/AuthProvider';
import { ThemeProvider } from '../context/ThemeContext';
import { ToastProvider } from '../components/feedback/Toast';
import { Sidebar } from '../features/navigation/Sidebar';
import { Topbar } from '../features/navigation/Topbar';
import { LoginPage } from '../features/auth/pages/LoginPage';
import { Router } from './routes';
import { ViewState } from '../types';

const Main = () => {
  const { user, isLoading } = useAuth();
  const [currentView, setCurrentView] = useState<ViewState>('records');
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  if (isLoading) return <div className="flex h-screen items-center justify-center"><Loader2 className="animate-spin" /></div>;
  if (!user) return <LoginPage />;

  const contentViews: ViewState[] = [
    'dashboard', 'collections', 'collections-create', 'users', 'files', 'logs', 'settings'
  ];
  const isContentView = contentViews.includes(currentView);

  return (
    <div className="flex min-h-screen bg-background text-foreground font-sans">
      <Sidebar 
        currentView={currentView} 
        onChangeView={setCurrentView} 
        isOpen={isMobileMenuOpen} 
        onClose={() => setIsMobileMenuOpen(false)} 
      />
      <div className="flex-1 flex flex-col h-screen overflow-hidden">
        <Topbar onMenuClick={() => setIsMobileMenuOpen(true)} />
        <main className="flex-1 overflow-y-auto bg-secondary/10">
          {isContentView ? (
            <div className="w-full max-w-screen-xl mx-auto p-4 sm:p-6">
              <Router view={currentView} onChangeView={setCurrentView} />
            </div>
          ) : (
            <Router view={currentView} onChangeView={setCurrentView} />
          )}
        </main>
      </div>
    </div>
  );
};

const App = () => {
  return (
    <ThemeProvider defaultTheme="dark" storageKey="tinybase-theme">
      <AuthProvider>
        <ToastProvider>
          <Main />
        </ToastProvider>
      </AuthProvider>
    </ThemeProvider>
  );
};

export default App;
