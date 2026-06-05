import React from 'react';
import { ChevronRight, Home } from 'lucide-react';
import { ViewState } from '../../types';

interface BreadcrumbItem {
  label: string;
  view: ViewState;
}

interface BreadcrumbProps {
  items: BreadcrumbItem[];
  onNavigate: (view: ViewState) => void;
}

export const Breadcrumb = ({ items, onNavigate }: BreadcrumbProps) => {
  return (
    <nav className="flex items-center text-sm font-medium text-muted-foreground">
      <button
        onClick={() => onNavigate('dashboard')}
        className="flex items-center gap-1.5 hover:text-primary transition-colors"
      >
        <Home className="h-4 w-4" />
      </button>
      {items.slice(1).map((item, index) => (
        <React.Fragment key={index}>
          <ChevronRight className="h-4 w-4 mx-1" />
          <button
            onClick={() => onNavigate(item.view)}
            className={`capitalize ${index === items.length - 2 ? 'text-foreground' : 'hover:text-primary transition-colors'}`}
          >
            {item.label}
          </button>
        </React.Fragment>
      ))}
    </nav>
  );
};
