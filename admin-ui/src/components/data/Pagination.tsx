import React from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { Button } from '../form/FormPrimitives';

interface PaginationProps {
  page: number;
  totalPages: number;
  onPageChange: (page: number) => void;
}

export const Pagination = ({ page, totalPages, onPageChange }: PaginationProps) => {
  return (
    <div className="flex items-center gap-2">
      <span className="text-sm text-muted-foreground mr-2">
        Page {page} of {totalPages}
      </span>
      <Button
        variant="outline"
        size="icon"
        disabled={page <= 1}
        onClick={() => onPageChange(page - 1)}
      >
        <ChevronLeft className="h-4 w-4" />
      </Button>
      <Button
        variant="outline"
        size="icon"
        disabled={page >= totalPages}
        onClick={() => onPageChange(page + 1)}
      >
        <ChevronRight className="h-4 w-4" />
      </Button>
    </div>
  );
};
