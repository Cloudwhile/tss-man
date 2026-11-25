import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import { AppProvider, useApp } from './context/AppContext';
import { I18nProvider, useI18n } from './i18n';
import { AppShell } from './components/AppShell';
import { AutomationPage } from './pages/AutomationPage';
import { DashboardPage } from './pages/DashboardPage';
import { GovernancePage } from './pages/GovernancePage';
import { InstancesPage } from './pages/InstancesPage';
import { LoginPage } from './pages/LoginPage';
import { PermissionsPage } from './pages/PermissionsPage';
import { SetupWizardPage } from './pages/SetupWizardPage';
import { SnapshotsPage } from './pages/SnapshotsPage';
import { UsersSecurityPage } from './pages/UsersSecurityPage';
import { WorkbenchPage } from './pages/WorkbenchPage';

function AppRoutes() {
  const { t } = useI18n();
  const { booting, initialized, user } = useApp();

  if (booting) {
    return (
      <div className="min-h-screen bg-surface text-text grid place-items-center text-sm text-text-muted">
        {t.common.loading}
      </div>
    );
  }

  if (!initialized) {
    return <SetupWizardPage />;
  }

  if (!user) {
    return <LoginPage />;
  }

  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/instances" element={<InstancesPage />} />
        <Route path="/workbench" element={<WorkbenchPage />} />
        <Route path="/permissions" element={<PermissionsPage />} />
        <Route path="/governance" element={<GovernancePage />} />
        <Route path="/snapshots" element={<SnapshotsPage />} />
        <Route path="/automation" element={<AutomationPage />} />
        <Route path="/security" element={<UsersSecurityPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}

export default function App() {
  return (
    <I18nProvider>
      <AppProvider>
        <BrowserRouter>
          <AppRoutes />
        </BrowserRouter>
      </AppProvider>
    </I18nProvider>
  );
}
