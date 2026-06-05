import React from 'react';
import { AdminUser } from '../../../types';

interface UserAvatarProps {
  user: AdminUser;
  className?: string;
}

export const UserAvatar = ({ user, className }: UserAvatarProps) => {
  const initials = user.email
    .split('@')[0]
    .split(/[\._-]/)
    .map((n) => n[0])
    .slice(0, 2)
    .join('')
    .toUpperCase();

  return (
    <div
      className={`relative inline-flex h-10 w-10 items-center justify-center rounded-full bg-secondary text-secondary-foreground ${className}`}
    >
      {user.avatar ? (
        <img
          src={user.avatar}
          alt={user.email}
          className="h-full w-full rounded-full object-cover"
        />
      ) : (
        <span className="font-medium">{initials}</span>
      )}
    </div>
  );
};
