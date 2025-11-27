
import React, { useState, useEffect, useCallback } from 'react';
import { Plus, Edit, Trash2 } from 'lucide-react';
import { Button } from '../../../components/form/FormPrimitives';
import { DataGrid } from '../../../components/data/DataGrid';
import { usersService } from '../services/usersService';
import { AdminUser } from '../../../types';
import { useToast } from '../../../components/feedback/Toast';
import { ConfirmDialog } from '../../../components/feedback/ConfirmDialog';
import { UserAvatar } from '../components/UserAvatar';
import { UserFormPanel } from '../components/UserFormPanel';

export const UsersListPage = () => {
  const [users, setUsers] = useState<AdminUser[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [editingUser, setEditingUser] = useState<AdminUser | null>(null);
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [deletingUser, setDeletingUser] = useState<AdminUser | null>(null);
  
  const { toast } = useToast();

  const fetchUsers = useCallback(async () => {
    setIsLoading(true);
    try {
      const userList = await usersService.list();
      setUsers(userList);
    } catch (error) {
      toast('Failed to fetch users', 'error');
    } finally {
      setIsLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    fetchUsers();
  }, [fetchUsers]);

  const handleSave = async (data: Partial<AdminUser>) => {
    try {
      if (editingUser) {
        await usersService.update(editingUser.id, data);
        toast('User updated successfully', 'success');
      } else {
        await usersService.create(data);
        toast('User created successfully', 'success');
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
          toast('User deleted successfully', 'success');
          setDeletingUser(null);
          fetchUsers();
      } catch (e) {
          toast('Failed to delete user', 'error');
      }
  }

  const columns = [
    {
      field: 'email',
      headerName: 'User',
      renderCell: (user: AdminUser) => (
        <div className="flex items-center gap-3">
          <UserAvatar user={user} />
          <div className="flex flex-col">
            <span className="font-medium">{user.email}</span>
            <span className="text-xs text-muted-foreground font-mono">{user.id}</span>
          </div>
        </div>
      ),
    },
    {
      field: 'lastActive',
      headerName: 'Last Active',
      renderCell: (user: AdminUser) => (
        <span className="text-sm text-muted-foreground hidden sm:inline">
          {new Date(user.lastActive).toLocaleString()}
        </span>
      ),
    },
    {
        field: 'actions',
        headerName: '',
        width: '120px',
        align: 'right' as const,
        renderCell: (user: AdminUser) => (
            <div className="flex justify-end gap-2">
                <Button variant="outline" size="sm" className="h-8 px-2 sm:px-3" onClick={(e) => { e.stopPropagation(); setEditingUser(user); setIsFormOpen(true); }}>
                    <Edit className="h-3 w-3 sm:mr-2" /> <span className="hidden sm:inline">Edit</span>
                </Button>
                <Button variant="ghost" size="icon" className="h-8 w-8 text-muted-foreground hover:text-destructive" onClick={(e) => { e.stopPropagation(); setDeletingUser(user); }}>
                    <Trash2 className="h-4 w-4" />
                </Button>
            </div>
        )
    }
  ];

  return (
    <div className="space-y-6">
      <div className="flex flex-col items-start justify-between gap-4 sm:flex-row sm:items-center">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Admin Users</h2>
          <p className="text-muted-foreground">Manage dashboard administrators and their permissions.</p>
        </div>
        <Button onClick={() => { setEditingUser(null); setIsFormOpen(true); }}>
          <Plus className="mr-2 h-4 w-4" /> New User
        </Button>
      </div>

      <DataGrid 
        data={users} 
        columns={columns} 
        keyField="id" 
        isLoading={isLoading} 
      />

      {isFormOpen && (
        <UserFormPanel 
            user={editingUser || undefined}
            onSave={handleSave}
            onCancel={() => setIsFormOpen(false)}
        />
      )}

      <ConfirmDialog 
        isOpen={!!deletingUser}
        title="Delete User"
        description={`Are you sure you want to delete ${deletingUser?.email}? This action cannot be undone.`}
        onConfirm={handleDelete}
        onCancel={() => setDeletingUser(null)}
        variant="destructive"
        confirmText="Delete"
      />
    </div>
  );
};
