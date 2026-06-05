// =========================== /teamspace/studios/this_studio/apex/apex-kit/admin-ui/src/features/users/pages/UsersListPage.tsx ===========================
import React, { useState, useEffect, useCallback } from 'react';
import { Plus, Edit, Trash2, Users, ShieldCheck, Search, Code, RefreshCw } from 'lucide-react';
import { Button, Input, Badge, Card, CardContent } from '../../../components/ui/Elements';
import { DataGrid } from '../../../components/data/DataGrid';
import { Pagination } from '../../../components/data/Pagination';
import { usersService } from '../services/usersService';
import { AuthUser } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { UserAvatar } from '../components/UserAvatar';
import { UserFormPanel } from '../components/UserFormPanel';
import { ApiDocsModal } from '../../records/components/ApiDocsModal';
import { usePagination } from '../../../hooks/usePagination';

export const UsersListPage = () => {
  const [users, setUsers] = useState<AuthUser[]>([]);
  const [search, setSearch] = useState('');
  const [isLoading, setIsLoading] = useState(true);
  const [editingUser, setEditingUser] = useState<AuthUser | null>(null);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [deletingUser, setDeletingUser] = useState<AuthUser | null>(null);

  // Pagination State
  const { page, perPage, setPage } = usePagination(1, 20);
  const [totalItems, setTotalItems] = useState(0);

  // Docs State
  const [isDocsOpen, setIsDocsOpen] = useState(false);

  const { toast } = useToast();

  const fetchUsers = useCallback(async () => {
    setIsLoading(true);
    try {
      const res = await usersService.list(page, perPage, search);
      setUsers(res.items);
      setTotalItems(res.total);
    } catch (error) {
      toast('Failed to fetch users', 'error');
    } finally {
      setIsLoading(false);
    }
  }, [page, perPage, search, toast]);

  // Debounce Search
  useEffect(() => {
    const timer = setTimeout(() => {
      setPage(1); // Reset to page 1 on search
      fetchUsers();
    }, 500);
    return () => clearTimeout(timer);
  }, [search]); // Trigger when search changes

  // Trigger when page changes
  useEffect(() => {
    fetchUsers();
  }, [page, fetchUsers]);

  const handleSave = async (data: Partial<AuthUser>) => {
    try {
      if (editingUser) {
        await usersService.update(editingUser.id, data);
        toast('User updated', 'success');
      } else {
        await usersService.create(data);
        toast('User created', 'success');
      }
      setIsFormOpen(false);
      fetchUsers();
    } catch (error) {
      toast('Failed to save user', 'error');
    }
  };

  const handleDelete = async () => {
    if (!deletingUser) return;
    try {
      await usersService.delete(deletingUser.id);
      toast('User deleted', 'success');
      setDeletingUser(null);
      fetchUsers();
    } catch (e) {
      toast('Failed to delete user', 'error');
    }
  };

  const columns = [
    {
      field: 'email',
      headerName: 'User Identity',
      renderCell: (user: AuthUser) => (
        <div className="flex items-center gap-3 py-1">
          <UserAvatar user={user} className="h-9 w-9 border border-border" />
          <div className="flex flex-col">
            <span className="font-medium text-foreground">{user.email}</span>
            <span className="text-[10px] text-muted-foreground font-mono">ID: {user.id}</span>
          </div>
        </div>
      ),
    },
    {
      field: 'role',
      headerName: 'Role',
      width: '120px',
      renderCell: (user: AuthUser) => {
        const isSystem = user.role === 'admin';
        return (
          <Badge
            variant={isSystem ? 'primary' : 'secondary'}
            className={`capitalize ${isSystem ? 'bg-purple-500/10 text-purple-400 border-purple-500/20' : ''}`}
          >
            {user.role}
          </Badge>
        );
      },
    },
    {
      field: 'lastActive',
      headerName: 'Last Active',
      width: '180px',
      renderCell: (user: AuthUser) => (
        <span className="text-xs text-muted-foreground font-mono">
          {new Date(user.lastActive).toLocaleDateString()}
        </span>
      ),
    },
    {
      field: 'actions',
      headerName: '',
      width: '100px',
      align: 'right' as const,
      renderCell: (user: AuthUser) => (
        <div className="flex justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-muted-foreground hover:text-primary"
            onClick={(e) => {
              e.stopPropagation();
              setEditingUser(user);
              setIsFormOpen(true);
            }}
          >
            <Edit className="h-3.5 w-3.5" />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-muted-foreground hover:text-destructive"
            onClick={(e) => {
              e.stopPropagation();
              setDeletingUser(user);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6 max-w-7xl mx-auto pb-20">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="space-y-1">
          <h2 className="text-3xl font-extrabold tracking-tight flex items-center gap-3">
            <Users className="h-8 w-8 text-primary" /> User Management
          </h2>
          <p className="text-muted-foreground text-sm md:text-base">
            Control access to your application. Manage roles and audit user activity.
          </p>
        </div>
        <div className="flex gap-2">
          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search users..."
              className="pl-9 w-full md:w-64 bg-background/50"
              value={search}
              onChange={(e: any) => setSearch(e.target.value)}
            />
          </div>
          <Button
            onClick={() => {
              setEditingUser(null);
              setIsFormOpen(true);
            }}
            className="shadow-lg"
          >
            <Plus className="mr-2 h-4 w-4" /> Add User
          </Button>
        </div>
      </div>

      {/* Toolbar */}
      <div className="flex items-center justify-between pt-2">
        <div className="flex gap-4">
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Users className="h-4 w-4" /> Total:{' '}
            <span className="font-bold text-foreground">{totalItems}</span>
          </div>
        </div>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" onClick={() => fetchUsers()} title="Refresh List">
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
          <Button variant="outline" size="sm" className="gap-2" onClick={() => setIsDocsOpen(true)}>
            <Code className="h-4 w-4" /> API Docs
          </Button>
        </div>
      </div>

      <div className="rounded-xl border border-border bg-card/50 backdrop-blur-sm overflow-hidden shadow-sm flex flex-col min-h-[400px]">
        <div className="flex-1">
          <DataGrid data={users} columns={columns} keyField="id" isLoading={isLoading} />
        </div>
        <div className="border-t border-border p-3 bg-background/50 flex justify-end">
          <Pagination
            page={page}
            totalPages={Math.ceil(totalItems / perPage) || 1}
            onPageChange={setPage}
          />
        </div>
      </div>

      {isFormOpen && (
        <UserFormPanel
          user={editingUser || undefined}
          onSave={handleSave}
          onCancel={() => setIsFormOpen(false)}
        />
      )}

      <ConfirmDialog
        isOpen={!!deletingUser}
        title="Delete User Account?"
        description={`Are you sure you want to delete ${deletingUser?.email}? This will revoke their access immediately.`}
        onConfirm={handleDelete}
        onCancel={() => setDeletingUser(null)}
        variant="destructive"
        confirmText="Delete Account"
      />

      <ApiDocsModal
        isOpen={isDocsOpen}
        onClose={() => setIsDocsOpen(false)}
        context="users" // Activate User Context
      />
    </div>
  );
};
