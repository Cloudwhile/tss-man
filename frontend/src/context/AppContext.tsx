import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import {
  apiRequest,
  AuthUserProfile,
  getAccessToken,
  InstanceView,
  restoreSession,
  setAccessToken,
} from '../lib/api';

export type ThemeChoice = 'system' | 'light' | 'dark';

export interface WsMessage {
  event: string;
  timestamp: string;
  instance_id: string;
  virtual_server_id: number;
  payload: Record<string, unknown>;
}

interface AppContextValue {
  booting: boolean;
  initialized: boolean;
  user: AuthUserProfile | null;
  instances: InstanceView[];
  activeInstanceId: string;
  activeSid: number;
  wsConnected: boolean;
  theme: ThemeChoice;
  setTheme: (next: ThemeChoice) => void;
  setInitializedAndUser: (user: AuthUserProfile | null, token?: string | null) => Promise<void>;
  logout: () => Promise<void>;
  refreshInstances: () => Promise<InstanceView[]>;
  setActiveInstanceId: (id: string) => void;
  setActiveSid: (sid: number) => void;
  subscribeWs: (handler: (msg: WsMessage) => void) => () => void;
}

const AppContext = createContext<AppContextValue | null>(null);

function applyThemeToDocument(theme: ThemeChoice) {
  const meta = document.querySelector('meta[name="color-scheme"]') as HTMLMetaElement | null;
  if (theme === 'system') {
    if (meta) meta.content = 'light dark';
    delete document.documentElement.dataset.theme;
  } else {
    if (meta) meta.content = theme;
    document.documentElement.dataset.theme = theme;
  }
}

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [booting, setBooting] = useState(true);
  const [initialized, setInitialized] = useState(true);
  const [user, setUser] = useState<AuthUserProfile | null>(null);
  const [instances, setInstances] = useState<InstanceView[]>([]);
  const [activeInstanceId, setActiveInstanceId] = useState<string>('');
  const [activeSid, setActiveSid] = useState<number>(1);
  const [wsConnected, setWsConnected] = useState(false);

  const [theme, setThemeState] = useState<ThemeChoice>(() => {
    try {
      const stored = localStorage.getItem('color-scheme');
      if (stored === 'light' || stored === 'dark') return stored;
    } catch {}
    return 'system';
  });

  const wsListenersRef = useRef<Set<(msg: WsMessage) => void>>(new Set());

  const setTheme = useCallback((next: ThemeChoice) => {
    setThemeState(next);
    applyThemeToDocument(next);
    try {
      if (next === 'system') {
        localStorage.removeItem('color-scheme');
      } else {
        localStorage.setItem('color-scheme', next);
      }
    } catch {}
  }, []);

  // Cross-tab color-scheme sync per
  useEffect(() => {
    applyThemeToDocument(theme);
    const onStorage = (e: StorageEvent) => {
      if (e.key === 'color-scheme') {
        const next: ThemeChoice =
          e.newValue === 'light' || e.newValue === 'dark' ? e.newValue : 'system';
        setThemeState(next);
        applyThemeToDocument(next);
      }
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, [theme]);

  const refreshInstances = useCallback(async (): Promise<InstanceView[]> => {
    try {
      const list = await apiRequest<InstanceView[]>('/instances');
      setInstances(list);
      setActiveInstanceId((prev) => {
        if (prev && list.some((i) => i.id === prev)) return prev;
        if (list.length > 0) {
          setActiveSid(list[0].default_virtual_server_id || 1);
          return list[0].id;
        }
        return '';
      });
      return list;
    } catch {
      return [];
    }
  }, []);

  useEffect(() => {
    let active = true;
    restoreSession()
      .then(async (res) => {
        if (!active) return;
        setInitialized(res.initialized);
        setUser(res.user);
        if (res.user) {
          await refreshInstances();
        }
      })
      .finally(() => {
        if (active) setBooting(false);
      });
    return () => {
      active = false;
    };
  }, [refreshInstances]);

  // WebSocket live connection
  useEffect(() => {
    if (!user) {
      setWsConnected(false);
      return;
    }
    let closed = false;
    let ws: WebSocket | null = null;
    let reconnectTimer: number | undefined;
    let pingTimer: number | undefined;

    const connect = () => {
      const token = getAccessToken();
      if (!token || closed) return;
      const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const url = `${proto}//${window.location.host}/api/v1/ws?token=${encodeURIComponent(token)}`;
      ws = new WebSocket(url);

      ws.onopen = () => {
        setWsConnected(true);
        pingTimer = window.setInterval(() => {
          if (ws?.readyState === WebSocket.OPEN) {
            ws.send('ping');
          }
        }, 25000);
      };

      ws.onmessage = (evt) => {
        try {
          const parsed = JSON.parse(evt.data) as WsMessage;
          if (parsed.event === 'pong') return;
          if (parsed.event === 'instance.status' || parsed.event === 'state.delta') {
            setInstances((prev) =>
              prev.map((inst) =>
                inst.id === parsed.instance_id && typeof parsed.payload?.status === 'string'
                  ? { ...inst, status: parsed.payload.status as string }
                  : inst,
              ),
            );
          }
          wsListenersRef.current.forEach((fn) => fn(parsed));
        } catch {}
      };

      ws.onclose = () => {
        setWsConnected(false);
        if (pingTimer) window.clearInterval(pingTimer);
        if (!closed) {
          reconnectTimer = window.setTimeout(connect, 2500);
        }
      };
    };

    connect();
    return () => {
      closed = true;
      if (pingTimer) window.clearInterval(pingTimer);
      if (reconnectTimer) window.clearTimeout(reconnectTimer);
      if (ws) ws.close();
    };
  }, [user]);

  const subscribeWs = useCallback((handler: (msg: WsMessage) => void) => {
    wsListenersRef.current.add(handler);
    return () => {
      wsListenersRef.current.delete(handler);
    };
  }, []);

  const setInitializedAndUser = useCallback(
    async (nextUser: AuthUserProfile | null, token?: string | null) => {
      if (token !== undefined) {
        setAccessToken(token);
      }
      setInitialized(true);
      setUser(nextUser);
      if (nextUser) {
        await refreshInstances();
      } else {
        setInstances([]);
        setActiveInstanceId('');
      }
    },
    [refreshInstances],
  );

  const logout = useCallback(async () => {
    try {
      await apiRequest('/auth/logout', { method: 'POST' }, false);
    } catch {}
    setAccessToken(null);
    setUser(null);
    setInstances([]);
    setActiveInstanceId('');
  }, []);

  return (
    <AppContext.Provider
      value={{
        booting,
        initialized,
        user,
        instances,
        activeInstanceId,
        activeSid,
        wsConnected,
        theme,
        setTheme,
        setInitializedAndUser,
        logout,
        refreshInstances,
        setActiveInstanceId: (id: string) => {
          setActiveInstanceId(id);
          const found = instances.find((i) => i.id === id);
          if (found) setActiveSid(found.default_virtual_server_id || 1);
        },
        setActiveSid,
        subscribeWs,
      }}
    >
      {children}
    </AppContext.Provider>
  );
}

export function useApp() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error('useApp must be used inside AppProvider');
  return ctx;
}
