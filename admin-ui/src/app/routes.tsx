
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
import { Dashboard } from '../pages/Dashboard';
import { ViewState } from '../types';

interface RouterProps {
    view: ViewState;
    onChangeView: (v: ViewState) => void;
}

export const Router = ({ view, onChangeView }: RouterProps) => {
    switch(view) {
        case 'dashboard': return <Dashboard />;
        case 'collections': return <CollectionsListPage onCreate={() => onChangeView('collections-create')} onEdit={() => onChangeView('collections-edit')} />;
        case 'collections-create': return <CollectionCreatePage onCancel={() => onChangeView('collections')} onSuccess={() => onChangeView('collections')} />;
        case 'collections-edit': return <CollectionEditPage onCancel={() => onChangeView('collections')} onSuccess={() => onChangeView('collections')} />;
        case 'records': return <RecordsListPage />;
        case 'files': return <FilesPage />;
        case 'settings': return <SettingsPage />;
        case 'logs': return <LogsDashboardPage />;
        case 'users': return <UsersListPage />;
        case 'scripts': return <ScriptsPage />;
        default: return <Dashboard />;
    }
};