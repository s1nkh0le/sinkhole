/* eslint-disable react-refresh/only-export-components */
import React, { createContext, useContext, useState, useCallback, useEffect } from 'react';
import { setAuthToken, setOnUnauthorized } from '../api/client';

interface AuthContextValue {
  token: string | null;
  isAuthenticated: boolean;
  login: (token: string) => Promise<boolean>;
  logout: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

const SESSION_KEY = 'sinkhole_auth_token';

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [token, setToken] = useState<string | null>(() => {
    return sessionStorage.getItem(SESSION_KEY);
  });

  const logout = useCallback(() => {
    setToken(null);
    setAuthToken(null);
    sessionStorage.removeItem(SESSION_KEY);
    sessionStorage.removeItem('sinkhole_traffic_filter');
  }, []);

  const login = useCallback(async (newToken: string): Promise<boolean> => {
    // We need to get the controller URL from settings. Since this context
    // doesn't depend on settings context, read from localStorage directly.
    let controllerUrl = window.location.origin;
    try {
      const stored = localStorage.getItem('sinkhole_settings');
      if (stored) {
        const parsed = JSON.parse(stored);
        if (parsed.controllerUrl) controllerUrl = parsed.controllerUrl;
      }
    } catch { /* ignore */ }

    try {
      const stateResponse = await fetch(`${controllerUrl}/state`, {
        headers: { 'Authorization': `Bearer ${newToken}` },
      });
      if (stateResponse.ok) {
        setToken(newToken);
        setAuthToken(newToken);
        sessionStorage.setItem(SESSION_KEY, newToken);
        sessionStorage.removeItem('sinkhole_traffic_filter');
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }, []);

  // Any API call that comes back 401/403 should drop us back to the login page.
  // Registered here (rather than at render time) so it is wired up exactly once.
  useEffect(() => {
    setOnUnauthorized(logout);
  }, [logout]);

  return (
    <AuthContext.Provider value={{ token, isAuthenticated: !!token, login, logout }}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}
