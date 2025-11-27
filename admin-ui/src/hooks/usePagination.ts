
import { useState } from 'react';

export const usePagination = (initialPage = 1, initialPerPage = 20) => {
  const [page, setPage] = useState(initialPage);
  const [perPage, setPerPage] = useState(initialPerPage);

  const nextPage = () => setPage(p => p + 1);
  const prevPage = () => setPage(p => Math.max(1, p - 1));
  const goToPage = (p: number) => setPage(p);

  return {
    page,
    perPage,
    setPage: goToPage,
    setPerPage,
    nextPage,
    prevPage
  };
};
