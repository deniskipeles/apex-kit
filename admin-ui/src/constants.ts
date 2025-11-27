
import { Collection, CollectionType, AppRecord, SystemLog, AdminUser, StoredFile } from './types';

export const MOCK_COLLECTIONS: Collection[] = [
  {
    id: 'col_users',
    name: 'users',
    type: CollectionType.AUTH,
    created: '2023-01-01T10:00:00Z',
    updated: '2023-06-15T14:30:00Z',
    schema: [
      { name: 'username', type: 'text', required: true },
      { name: 'email', type: 'email', required: true },
      { name: 'avatar', type: 'file', required: false },
      { name: 'verified', type: 'bool', required: false }
    ]
  },
  {
    id: 'col_posts',
    name: 'posts',
    type: CollectionType.BASE,
    created: '2023-02-10T09:15:00Z',
    updated: '2023-02-10T09:15:00Z',
    schema: [
      { name: 'title', type: 'text', required: true },
      { name: 'slug', type: 'text', required: true },
      { name: 'content', type: 'json', required: false }, 
      { name: 'published', type: 'bool', required: false },
      { name: 'author', type: 'relation', required: true, relationTo: 'col_users' }
    ]
  },
  {
    id: 'col_comments',
    name: 'comments',
    type: CollectionType.BASE,
    created: '2023-03-05T11:20:00Z',
    updated: '2023-03-05T11:20:00Z',
    schema: [
      { name: 'text', type: 'text', required: true },
      { name: 'post_id', type: 'relation', required: true, relationTo: 'col_posts' },
      { name: 'user_id', type: 'relation', required: true, relationTo: 'col_users' }
    ]
  }
];

export const MOCK_RECORDS: AppRecord[] = [
  {
    id: 'rec_u1',
    collectionId: 'col_users',
    collectionName: 'users',
    created: '2023-01-02T08:00:00Z',
    updated: '2023-01-02T08:00:00Z',
    username: 'admin_dave',
    email: 'dave@tinybase.io',
    verified: true,
    avatar: 'users/dave.jpg'
  },
  {
    id: 'rec_u2',
    collectionId: 'col_users',
    collectionName: 'users',
    created: '2023-01-05T12:00:00Z',
    updated: '2023-01-05T12:00:00Z',
    username: 'sarah_connor',
    email: 'sarah@resistance.net',
    verified: false,
    avatar: ''
  },
  {
    id: 'rec_p1',
    collectionId: 'col_posts',
    collectionName: 'posts',
    created: '2023-02-12T10:00:00Z',
    updated: '2023-02-12T10:00:00Z',
    title: 'Welcome to Tinybase',
    slug: 'welcome-tinybase',
    published: true,
    author: 'rec_u1',
    content: { type: 'doc', content: [{ type: 'paragraph', text: 'Hello world' }] }
  },
  {
    id: 'rec_p2',
    collectionId: 'col_posts',
    collectionName: 'posts',
    created: '2023-02-15T14:00:00Z',
    updated: '2023-02-15T14:00:00Z',
    title: 'Advanced Querying 101',
    slug: 'advanced-querying',
    published: true,
    author: 'rec_u1',
    content: { type: 'doc', content: [] }
  }
];

export const MOCK_ADMIN_USERS: AdminUser[] = [
    { id: 'admin_1', email: 'admin@tinybase.io', lastActive: new Date(Date.now() - 1000 * 60 * 5).toISOString(), avatar: 'https://i.pravatar.cc/150?u=admin_1' },
    { id: 'admin_2', email: 'jane.doe@tinybase.io', lastActive: new Date(Date.now() - 1000 * 60 * 60 * 24).toISOString(), avatar: 'https://i.pravatar.cc/150?u=admin_2' },
    { id: 'admin_3', email: 'sam.smith@tinybase.io', lastActive: new Date(Date.now() - 1000 * 60 * 60 * 72).toISOString(), avatar: 'https://i.pravatar.cc/150?u=admin_3' }
];

export const MOCK_LOGS: SystemLog[] = [
  { id: 'log_1', level: 'info', message: 'Server started on port 8090', timestamp: new Date(Date.now() - 1000000).toISOString(), source: 'system' },
  { id: 'log_2', level: 'success', message: 'Database migration v2 applied', timestamp: new Date(Date.now() - 800000).toISOString(), source: 'db' },
  { id: 'log_3', level: 'warning', message: 'High memory usage detected', timestamp: new Date(Date.now() - 500000).toISOString(), source: 'monitor' },
  { id: 'log_4', level: 'error', message: 'Failed login attempt: unknown user', timestamp: new Date(Date.now() - 100000).toISOString(), source: 'auth' },
];

export const CHART_DATA = [
  { name: 'Mon', requests: 4000, errors: 240 },
  { name: 'Tue', requests: 3000, errors: 139 },
  { name: 'Wed', requests: 2000, errors: 980 },
  { name: 'Thu', requests: 2780, errors: 390 },
  { name: 'Fri', requests: 1890, errors: 480 },
  { name: 'Sat', requests: 2390, errors: 380 },
  { name: 'Sun', requests: 3490, errors: 430 },
];

export const MOCK_FILES: StoredFile[] = [
  { id: 'file_1', name: 'project-proposal.pdf', size: 1024 * 450, mimeType: 'application/pdf', url: '#', created: '2023-10-01T12:00:00Z', updated: '2023-10-01T12:00:00Z' },
  { id: 'file_2', name: 'banner-hero.jpg', size: 1024 * 2500, mimeType: 'image/jpeg', url: 'https://picsum.photos/800/400', created: '2023-10-05T09:30:00Z', updated: '2023-10-05T09:30:00Z' },
  { id: 'file_3', name: 'logo-transparent.png', size: 1024 * 120, mimeType: 'image/png', url: 'https://picsum.photos/200/200', created: '2023-10-06T14:15:00Z', updated: '2023-10-06T14:15:00Z' },
  { id: 'file_4', name: 'financial-report.xlsx', size: 1024 * 800, mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', url: '#', created: '2023-10-07T10:00:00Z', updated: '2023-10-07T10:00:00Z' },
  { id: 'file_5', name: 'team-meeting.mp4', size: 1024 * 1024 * 45, mimeType: 'video/mp4', url: '#', created: '2023-10-08T16:45:00Z', updated: '2023-10-08T16:45:00Z' },
  { id: 'file_6', name: 'avatar-default.svg', size: 1024 * 5, mimeType: 'image/svg+xml', url: 'https://picsum.photos/50/50', created: '2023-10-10T11:20:00Z', updated: '2023-10-10T11:20:00Z' },
];
