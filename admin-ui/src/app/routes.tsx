import React from 'react';
import { RecordsListPage } from '../features/records/pages/RecordsListPage';
import { CollectionsListPage } from '../features/collections/pages/CollectionsListPage';
import { CollectionCreatePage } from '../features/collections/pages/CollectionCreatePage';
import { CollectionEditPage } from '../features/collections/pages/CollectionEditPage';
import { FilesPage } from '../pages/FilesPage';
import { SettingsPage } from '../features/settings/pages/SettingsPage';
import { LogsDashboardPage } from '../features/logs/pages/LogsDashboardPage';
import { UsersListPage } from '../features/users/pages/UsersListPage';
import { ScriptsPage } from '../features/scripts/pages/ScriptsPage';
import { TemplatesPage } from '../features/templates/pages/TemplatesPage';
import { AiActionsPage } from '../features/ai/pages/AiActionsPage';
import { AiArchitectPage } from '../features/ai/pages/AiArchitectPage';
import { TenantsListPage } from '../features/tenants/pages/TenantsListPage';
import { VectorSearchPanel } from '../features/ai/components/VectorSearchPanel';
import { Dashboard } from '../pages/Dashboard';

interface RouterProps {
    view: string;
    onChangeView: (v: string) => void;
}

export const Router = ({ view, onChangeView }: any) => {
    let contextPrefix = '';
    let activeView = view;

    // FIX: Match the double underscore logic
    if (view.startsWith('tenant__')) {
        const parts = view.split('__');
        if (parts.length >= 3) {
            contextPrefix = `tenant__${parts[1]}__`;
            activeView = parts[2];
        }
    } else if (view.startsWith('sandbox__')) {
        const parts = view.split('__');
        if (parts.length >= 3) {
            contextPrefix = `sandbox__${parts[1]}__`;
            activeView = parts[2];
        }
    }

    const nav = (target: string) => onChangeView(contextPrefix + target);

    switch(activeView) {
        // --- Shared Views (Available in Root, Tenant, Sandbox) ---
        case 'dashboard': return <Dashboard />;
        
        case 'collections': return <CollectionsListPage onCreate={() => nav('collections-create')} onEdit={() => nav('collections-edit')} />;
        case 'collections-create': return <CollectionCreatePage onCancel={() => nav('collections')} onSuccess={() => nav('collections')} />;
        case 'collections-edit': return <CollectionEditPage onCancel={() => nav('collections')} onSuccess={() => nav('collections')} />;
        
        case 'records': return <RecordsListPage />;
        case 'files': return <FilesPage />;
        case 'settings': return <SettingsPage />;
        case 'logs': return <LogsDashboardPage />;
        
        // Note: In Tenant/Sandbox mode, this lists *that instance's* users/auth
        case 'users': return <UsersListPage />; 
        case 'scripts': return <ScriptsPage />;
        case 'templates': return <TemplatesPage />;
        case 'ai-actions': return <AiActionsPage />;
        case 'vector-search': return <VectorSearchPanel />;

        // --- Root Only Views ---
        // We hide these if in a specific context to prevent confusion, 
        // although the API would block them anyway.
        case 'tenants': return !contextPrefix ? <TenantsListPage /> : <Dashboard />;
        case 'ai-architect': return !contextPrefix ? <AiArchitectPage /> : <Dashboard />;

        default: return <Dashboard />;
    }
};