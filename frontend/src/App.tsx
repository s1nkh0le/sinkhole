import {
  createBrowserRouter,
  RouterProvider,
  Routes,
  Route,
  Navigate,
} from 'react-router-dom';
import { SettingsProvider } from './stores/SettingsContext';
import { AuthProvider, useAuth } from './stores/AuthContext';
import { ToastProvider } from './stores/ToastContext';
import { AppShell } from './layouts/AppShell';
import { LoginPage } from './pages/LoginPage';
import { ServicesPage } from './pages/ServicesPage';
import { ServiceDetailPage } from './pages/ServiceDetailPage';
import { PortDetailPage } from './pages/PortDetailPage';
import { TrafficAnalyzerPage } from './pages/TrafficAnalyzerPage';
import { EcamPage } from './pages/EcamPage';
import { SettingsPage } from './pages/SettingsPage';

/**
 * Layout route for the authenticated area. Sends the user to the login page
 * unless the session holds a token; otherwise renders the app chrome. Token
 * sync and 401 handling live in AuthProvider, so this stays a pure gate.
 */
function AuthGate() {
  const { isAuthenticated } = useAuth();
  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }
  return <AppShell />;
}

/** Route table. The authenticated pages render inside the AuthGate layout route. */
function AppRoutes() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<AuthGate />}>
        <Route path="/services" element={<ServicesPage />} />
        <Route path="/services/:serviceId" element={<ServiceDetailPage />} />
        <Route path="/services/:serviceId/ports/:port" element={<PortDetailPage />} />
        <Route path="/traffic" element={<TrafficAnalyzerPage />} />
        <Route path="/ecam" element={<EcamPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route index element={<Navigate to="/services" replace />} />
      </Route>
      <Route path="*" element={<Navigate to="/services" replace />} />
    </Routes>
  );
}

/**
 * Wraps all routes in the app-wide context providers. This is a single
 * catch-all layout route so the providers live inside RouterProvider and are
 * available to every child route.
 */
function RootLayout() {
  return (
    <SettingsProvider>
      <AuthProvider>
        <ToastProvider>
          <AppRoutes />
        </ToastProvider>
      </AuthProvider>
    </SettingsProvider>
  );
}

const router = createBrowserRouter(
  [
    {
      path: '*',
      Component: RootLayout,
    },
  ],
  // Served under /ui by the controller (Vite's base); the API stays at the origin root.
  { basename: import.meta.env.BASE_URL.replace(/\/$/, '') },
);

export default function App() {
  return <RouterProvider router={router} />;
}
