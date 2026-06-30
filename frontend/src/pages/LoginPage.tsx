import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '../stores/AuthContext';
import { useSettings } from '../stores/SettingsContext';
import { ConnectionConfigForm } from '../components/ConnectionConfigForm';
import './LoginPage.css';

/**
 * Entry screen: collects and verifies the controller URL and API token (via
 * ConnectionConfigForm), stores the URL in settings, logs in, and redirects to
 * the services overview on success.
 */
export function LoginPage() {
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const { login } = useAuth();
  const { settings, updateSettings } = useSettings();
  const navigate = useNavigate();

  const handleApplyConfig = async (newUrl: string, newToken: string) => {
    setLoading(true);
    setError('');

    // Update settings with the potentially new URL
    updateSettings({ ...settings, controllerUrl: newUrl });

    // Login using the provided token
    const ok = await login(newToken);
    setLoading(false);

    if (ok) {
      navigate('/services', { replace: true });
    } else {
      setError('Authentication failed. Check your token and controller URL.');
    }
  };

  return (
    <div className="login-page">
      <div className="login-page__card">
        <div className="login-page__header">
          <span className="login-page__logo">◉</span>
          <h1 className="login-page__title">sinkhole</h1>
          <p className="login-page__subtitle">Operations Console</p>
        </div>

        <ConnectionConfigForm
          initialUrl={settings.controllerUrl}
          onApply={handleApplyConfig}
          applyLabel="Sign In"
          loading={loading}
        />

        {error && <div className="login-page__error" style={{ marginTop: 'var(--space-4)' }}>{error}</div>}
      </div>
    </div>
  );
}
