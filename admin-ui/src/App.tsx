
import React from 'react';
import { ThemeProvider } from './context/ThemeContext';
import { ToastProvider } from './components/feedback/Toast';
import { MainLayout } from './app/layout/MainLayout';
import { APEX_THEME } from './constants';

const App = () => {
  return (
    <ThemeProvider defaultTheme="dark" storageKey={APEX_THEME}>
      <ToastProvider>
        <MainLayout />
      </ToastProvider>
    </ThemeProvider>
  );
};

export default App;
