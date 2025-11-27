
import React from 'react';
import { ThemeProvider } from './context/ThemeContext';
import { ToastProvider } from './components/feedback/Toast';
import { MainLayout } from './app/layout/MainLayout';

const App = () => {
  return (
    <ThemeProvider defaultTheme="dark" storageKey="tinybase-theme">
      <ToastProvider>
        <MainLayout />
      </ToastProvider>
    </ThemeProvider>
  );
};

export default App;
