
export * from './collection';
export * from './record';
export * from './user';
export * from './settings';
export * from './api';
export * from './files';


// NEW: Script Types
export interface Script {
    id: string;
    name: string;
    trigger_type: 'manual' | 'before_create' | 'after_create' | 'before_update' | 'after_update' | 'before_delete' | 'after_delete' | 'cron';
    code: string;
    active: boolean;
}