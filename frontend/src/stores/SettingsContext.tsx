/* eslint-disable react-refresh/only-export-components */
import React, { createContext, useContext, useState, useEffect, useCallback } from 'react';

interface Settings {
  controllerUrl: string;
  /** Default poll interval (fallback for anything not overridden) */
  pollIntervalMs: number;
  /** Interval for /state (services, ports, rulesets) */
  statePollMs: number;
  /** Interval for /health */
  healthPollMs: number;
  /** Interval for ECAM events */
  ecamPollMs: number;
  /** Interval for traffic records */
  recordsPollMs: number;
}

const DEFAULT_SETTINGS: Settings = {
  controllerUrl: window.location.origin,
  pollIntervalMs: 5000,
  statePollMs: 4000,
  healthPollMs: 2000,
  ecamPollMs: 10000,
  recordsPollMs: 15000,
};

const STORAGE_KEY = 'sinkhole_settings';

function loadSettings(): Settings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      return { ...DEFAULT_SETTINGS, ...parsed };
    }
  } catch {
    // ignore
  }
  return { ...DEFAULT_SETTINGS };
}

interface SettingsContextValue {
  settings: Settings;
  updateSettings: (patch: Partial<Settings>) => void;
  resetSettings: () => void;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

/**
 * Holds user-facing settings (controller URL and the per-resource poll
 * intervals) and persists them to localStorage, so they survive reloads and are
 * readable outside React (the API client reads the controller URL from the same
 * key). Consume via {@link useSettings}.
 */
export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [settings, setSettings] = useState<Settings>(loadSettings);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  }, [settings]);

  const updateSettings = useCallback((patch: Partial<Settings>) => {
    setSettings(prev => ({ ...prev, ...patch }));
  }, []);

  const resetSettings = useCallback(() => {
    setSettings({ ...DEFAULT_SETTINGS });
  }, []);

  return (
    <SettingsContext.Provider value={{ settings, updateSettings, resetSettings }}>
      {children}
    </SettingsContext.Provider>
  );
}

export function useSettings(): SettingsContextValue {
  const ctx = useContext(SettingsContext);
  if (!ctx) throw new Error('useSettings must be used within SettingsProvider');
  return ctx;
}
