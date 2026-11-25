import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { NavLink, Outlet } from 'react-router-dom';
import {
  Activity,
  Bot,
  FolderArchive,
  Languages,
  LayoutDashboard,
  LogOut,
  Menu,
  Monitor,
  Moon,
  Network,
  Radio,
  Server,
  ShieldAlert,
  ShieldCheck,
  Sun,
  UserCheck,
  X,
} from 'lucide-react';
import { ThemeChoice, useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import { apiRequest, VirtualServerSummary } from '../lib/api';
import { TssBrandMark } from './TssBrandMark';
import { SelectField, SelectOption } from './ui/SelectField';
import { StatusBadge } from './ui/StatusBadge';

export function AppShell() {
  const { t, locale, setLocale } = useI18n();
  const {
    user,
    instances,
    activeInstanceId,
    activeSid,
    wsConnected,
    theme,
    setTheme,
    setActiveInstanceId,
    setActiveSid,
    logout,
  } = useApp();

  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [vservers, setVservers] = useState<VirtualServerSummary[]>([]);

  const drawerId = useId();
  const drawerRef = useRef<HTMLDialogElement>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const sheetRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!activeInstanceId) {
      setVservers([]);
      return;
    }
    let alive = true;
    apiRequest<VirtualServerSummary[]>(`/instances/${activeInstanceId}/servers`)
      .then((list) => {
        if (alive) setVservers(list);
      })
      .catch(() => {
        if (alive) setVservers([]);
      });
    return () => {
      alive = false;
    };
  }, [activeInstanceId]);

  const openDrawer = useCallback(async () => {
    const dlg = drawerRef.current;
    const scroller = scrollerRef.current;
    if (!dlg || !scroller || dlg.open) return;

    setMobileNavOpen(true);
    dlg.showModal();

    if (typeof CSS === 'undefined' || !CSS.supports('scroll-initial-target', 'nearest')) {
      scroller.scrollTo({ left: scroller.scrollWidth, behavior: 'instant' as ScrollBehavior });
      await new Promise<void>((r) =>
        requestAnimationFrame(() => requestAnimationFrame(() => r())),
      );
    }
    scroller.scrollTo({ left: 0, behavior: 'auto' });
  }, []);

  const closeDrawer = useCallback(() => {
    const dlg = drawerRef.current;
    const scroller = scrollerRef.current;
    if (!dlg || !dlg.open) return;
    if (scroller) {
      scroller.scrollTo({ left: scroller.scrollWidth, behavior: 'auto' });
    } else {
      dlg.close();
      setMobileNavOpen(false);
    }
  }, []);

  useEffect(() => {
    const dlg = drawerRef.current;
    const scroller = scrollerRef.current;
    const sheet = sheetRef.current;
    if (!dlg || !scroller || !sheet) return;

    const threshold = 1 / Math.max(320, window.innerWidth);
    const observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[entries.length - 1];
        if (!entry) return;
        if (entry.intersectionRatio < threshold && dlg.open) {
          dlg.close();
          setMobileNavOpen(false);
        }
        if (entry.intersectionRatio >= 0.99) {
          setMobileNavOpen(true);
        }
      },
      { root: dlg, threshold: [threshold, 1] },
    );
    observer.observe(sheet);

    let removeScrollListener: (() => void) | undefined;
    if (typeof CSS === 'undefined' || !CSS.supports('animation-timeline', 'scroll()')) {
      const onScroll = () => {
        const width = Math.max(1, sheet.offsetWidth);
        const ratio = Math.max(0, Math.min(1, 1 - scroller.scrollLeft / width));
        dlg.style.setProperty('--drawer-backdrop', String(ratio));
      };
      scroller.addEventListener('scroll', onScroll, { passive: true });
      removeScrollListener = () => scroller.removeEventListener('scroll', onScroll);
    }

    const onClick = (e: MouseEvent) => {
      if (!sheet.contains(e.target as Node)) closeDrawer();
    };
    const onCancel = (e: Event) => {
      if (e.cancelable) {
        e.preventDefault();
        closeDrawer();
      }
    };
    const onClose = () => {
      setMobileNavOpen(false);
    };

    dlg.addEventListener('click', onClick);
    dlg.addEventListener('cancel', onCancel);
    dlg.addEventListener('close', onClose);

    return () => {
      observer.disconnect();
      removeScrollListener?.();
      dlg.removeEventListener('click', onClick);
      dlg.removeEventListener('cancel', onCancel);
      dlg.removeEventListener('close', onClose);
    };
  }, [closeDrawer]);

  const activeInstance = instances.find((i) => i.id === activeInstanceId) ?? null;
  const activeVServer = vservers.find((vs) => vs.id === activeSid) ?? null;

  const navGroups = [
    {
      title: t.nav.groupOps,
      items: [
        { to: '/', label: t.nav.dashboard, icon: LayoutDashboard },
        {
          to: '/instances',
          label: t.nav.instances,
          icon: Server,
          badge: instances.length > 0 ? String(instances.length) : undefined,
        },
        {
          to: '/workbench',
          label: t.nav.workbench,
          icon: Network,
          badge: activeVServer ? `${activeVServer.clients_online}` : undefined,
        },
      ],
    },
    {
      title: t.nav.groupGov,
      items: [
        { to: '/permissions', label: t.nav.permissions, icon: ShieldCheck },
        { to: '/governance', label: t.nav.governance, icon: ShieldAlert },
        { to: '/snapshots', label: t.nav.snapshots, icon: FolderArchive },
        { to: '/automation', label: t.nav.automation, icon: Bot },
        { to: '/security', label: t.nav.security, icon: UserCheck },
      ],
    },
  ];

  const initials = (user?.display_name || user?.username || 'OP')
    .trim()
    .slice(0, 2)
    .toUpperCase();

  const instanceOptions: SelectOption[] = useMemo(
    () =>
      instances.map((inst) => ({
        value: inst.id,
        label: inst.name,
        badge: inst.protocol === 'raw_tcp' ? 'TCP' : inst.protocol === 'ssh' ? 'SSH' : 'WEB',
        meta: `${inst.host}:${inst.port}`,
        statusDot:
          inst.status === 'online'
            ? 'online'
            : inst.status === 'connecting'
              ? 'connecting'
              : 'offline',
      })),
    [instances],
  );

  const vserverOptions: SelectOption[] = useMemo(
    () =>
      vservers.map((vs) => ({
        value: String(vs.id),
        label: vs.name,
        badge: `#${vs.id}`,
        meta: `${vs.clients_online}/${vs.max_clients}`,
        statusDot: vs.status === 'online' ? 'online' : 'offline',
      })),
    [vservers],
  );

  const renderNodeTelemetryPod = () => {
    if (!activeInstance) {
      return (
        <div className="mx-3 mt-3 p-3 tss-sidebar-card">
          <div className="flex items-center gap-2 text-xs text-[var(--sidebar-text-muted)]">
            <Radio className="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
            <span className="truncate">{t.dashboard.emptyTitle}</span>
          </div>
        </div>
      );
    }

    const used = activeVServer?.clients_online ?? 0;
    const max = Math.max(1, activeVServer?.max_clients ?? 1);
    const pct = Math.min(100, Math.round((used / max) * 100));
    const statusTone =
      activeInstance.status === 'online'
        ? 'bg-success'
        : activeInstance.status === 'connecting'
          ? 'bg-warning'
          : 'bg-danger';

    return (
      <div className="mx-3 mt-3 p-3 tss-sidebar-card space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[10.5px] font-semibold uppercase tracking-wider text-[var(--sidebar-text-muted)] tss-trim">
            {t.nav.nodeStatus}
          </span>
          <span className="font-mono text-[10px] font-medium uppercase px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-action-subtle text-action-text border border-action/20 leading-none tss-trim">
            {activeInstance.protocol === 'raw_tcp'
              ? 'RAW TCP'
              : activeInstance.protocol === 'ssh'
                ? 'SSH'
                : 'WEB'}
          </span>
        </div>
        <div className="flex items-center gap-2 min-w-0">
          <span className={`w-2 h-2 rounded-full shrink-0 ${statusTone}`} aria-hidden="true" />
          <span className="text-xs font-semibold text-[var(--sidebar-text-strong)] truncate">
            {activeInstance.name}
          </span>
        </div>
        <div className="flex items-center justify-between text-[11px] font-mono text-[var(--sidebar-text-muted)] tabular-nums">
          <span className="truncate">
            {activeInstance.host}:{activeInstance.port}
          </span>
          {activeVServer ? (
            <span className="shrink-0 font-medium text-[var(--sidebar-text-strong)]">
              {used}/{max}
            </span>
          ) : null}
        </div>
        {activeVServer ? (
          <div
            className="h-1 w-full rounded-full bg-border overflow-hidden"
            aria-hidden="true"
          >
            <div
              className="h-full rounded-full bg-action transition-[width] duration-300"
              style={{ width: `${pct}%` }}
            />
          </div>
        ) : null}
      </div>
    );
  };

  const renderNavLinks = (onNavigate?: () => void) => (
    <div className="flex-1 px-3 py-4 overflow-y-auto tss-scroller space-y-5">
      {navGroups.map((group) => (
        <div key={group.title}>
          <div className="px-2.5 pb-1.5 text-[10.5px] font-semibold uppercase tracking-wider text-[var(--sidebar-text-muted)]">
            {group.title}
          </div>
          <div className="space-y-0.5">
            {group.items.map((item) => {
              const Icon = item.icon;
              return (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.to === '/'}
                  onClick={onNavigate}
                  className="tss-nav-item"
                >
                  <span className="flex items-center gap-2.5 min-w-0">
                    <Icon
                      className="tss-nav-icon w-[15px] h-[15px] shrink-0"
                      strokeWidth={1.8}
                      aria-hidden="true"
                    />
                    <span className="truncate">{item.label}</span>
                  </span>
                  {item.badge ? (
                    <span className="font-mono text-[10.5px] px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken/80 border border-border/70 text-[var(--sidebar-text-muted)] tabular-nums leading-none shrink-0 tss-trim">
                      {item.badge}
                    </span>
                  ) : null}
                </NavLink>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );

  const operatorFooter = (
    <div className="p-3 border-t border-[var(--sidebar-divider)]">
      <div className="flex items-center justify-between gap-2 px-1.5 py-1">
        <div className="flex items-center gap-2.5 min-w-0">
          <div
            aria-hidden="true"
            className="w-8 h-8 rounded-[var(--radius-DEFAULT)] bg-action-subtle border border-action/20 text-xs font-semibold text-action-text grid place-items-center shrink-0 font-mono"
          >
            {initials}
          </div>
          <div className="min-w-0">
            <div className="text-xs font-semibold text-[var(--sidebar-text-strong)] truncate leading-tight">
              {user?.display_name}
            </div>
            <div className="mt-0.5 flex items-center gap-1.5 min-w-0">
              <span className="text-[11px] text-[var(--sidebar-text-muted)] font-mono truncate">
                @{user?.username}
              </span>
              <span className="text-[9.5px] font-mono font-medium uppercase px-1.5 py-0.5 rounded-[var(--radius-sm)] bg-surface-sunken border border-border text-text-secondary shrink-0 leading-none tss-trim">
                {user?.global_role === 'super_admin' ? 'ROOT' : 'MEMBER'}
              </span>
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={logout}
          title={t.common.logout}
          aria-label={t.common.logout}
          className="w-8 h-8 shrink-0 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] text-[var(--sidebar-text-muted)] hover:text-danger hover:bg-danger-subtle cursor-pointer transition-colors"
        >
          <LogOut className="w-4 h-4" strokeWidth={1.75} aria-hidden="true" />
        </button>
      </div>
    </div>
  );

  const themeOptions: { id: ThemeChoice; icon: typeof Monitor; label: string }[] = [
    { id: 'system', icon: Monitor, label: t.common.themeSystem },
    { id: 'light', icon: Sun, label: t.common.themeLight },
    { id: 'dark', icon: Moon, label: t.common.themeDark },
  ];

  return (
    <div className="min-h-screen flex bg-surface text-text">
      {/* Desktop Tactile Control Rail */}
      <aside className="hidden lg:block w-[244px] shrink-0 tss-sidebar">
        <div className="sticky top-0 h-screen flex flex-col">
          <div className="px-4 min-h-14 flex items-center justify-between gap-2 border-b border-[var(--sidebar-divider)]">
            <div className="flex items-center gap-2.5 min-w-0">
              <TssBrandMark className="w-7 h-7" />
              <div className="min-w-0">
                <div className="text-sm font-semibold text-[var(--sidebar-text-strong)] truncate leading-tight tracking-tight">
                  {t.appName}
                </div>
                <div className="text-[11px] text-[var(--sidebar-text-muted)] truncate leading-tight mt-0.5">
                  {t.appSubtitle}
                </div>
              </div>
            </div>
          </div>
          {renderNodeTelemetryPod()}
          <nav aria-label="Main Navigation" className="flex-1 flex flex-col min-h-0">
            {renderNavLinks()}
          </nav>
          {operatorFooter}
        </div>
      </aside>

      {/* Mobile Navigation Drawer (<dialog> + scroll-snap) */}
      <dialog
        ref={drawerRef}
        id={drawerId}
        aria-label="Main Navigation"
        className="tss-drawer lg:hidden"
      >
        <div ref={scrollerRef} className="tss-drawer-scroller">
          <nav
            ref={sheetRef}
            className="tss-drawer-sheet tss-sidebar shadow-[var(--shadow-xl)]"
          >
            <div className="px-4 min-h-14 flex items-center justify-between gap-2 border-b border-[var(--sidebar-divider)]">
              <div className="flex items-center gap-2.5 min-w-0">
                <TssBrandMark className="w-7 h-7" />
                <span className="text-sm font-semibold text-[var(--sidebar-text-strong)] truncate">
                  {t.appName}
                </span>
              </div>
              <button
                type="button"
                autoFocus
                onClick={closeDrawer}
                aria-label={t.common.close}
                className="w-8 h-8 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] text-[var(--sidebar-text-muted)] hover:text-[var(--sidebar-text-strong)] hover:bg-[var(--sidebar-hover-bg)] cursor-pointer"
              >
                <X className="w-4 h-4" aria-hidden="true" />
              </button>
            </div>
            {renderNodeTelemetryPod()}
            {renderNavLinks(closeDrawer)}
            {operatorFooter}
          </nav>
        </div>
      </dialog>

      {/* Main content area */}
      <div className="flex-1 min-w-0 flex flex-col">
        <header className="sticky top-0 z-[var(--z-sticky)] bg-surface-raised/85 backdrop-blur-md border-b border-border">
          <div className="min-h-14 px-4 lg:px-7 py-1.5 flex items-center justify-between gap-3">
            <div className="flex items-center gap-1.5 min-w-0 flex-1">
              <button
                type="button"
                onClick={openDrawer}
                aria-label="Open navigation"
                aria-expanded={mobileNavOpen}
                aria-controls={drawerId}
                className="lg:hidden w-8 h-8 shrink-0 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] border border-border-strong bg-surface-raised text-text hover:bg-surface cursor-pointer me-1"
              >
                <Menu className="w-4 h-4" aria-hidden="true" />
              </button>

              <div className="flex items-center gap-2 min-w-0 sm:hidden">
                <TssBrandMark className="w-7 h-7" />
                <span className="text-sm font-semibold text-text truncate">{t.appName}</span>
              </div>

              {instances.length > 0 ? (
                <div className="hidden sm:flex items-center gap-1 min-w-0 flex-1">
                  <div className="min-w-0 max-w-96">
                    <SelectField
                      id="global-instance-select"
                      ariaLabel={t.common.selectInstance}
                      variant="breadcrumb"
                      value={activeInstanceId}
                      options={instanceOptions}
                      onValueChange={setActiveInstanceId}
                    />
                  </div>

                  {vservers.length > 0 ? (
                    <>
                      <span
                        className="text-border-strong font-light text-base select-none px-0.5"
                        aria-hidden="true"
                      >
                        /
                      </span>
                      <div className="min-w-0 max-w-96">
                        <SelectField
                          id="global-vserver-select-desktop"
                          ariaLabel={t.common.selectVServer}
                          variant="breadcrumb"
                          value={String(activeSid)}
                          options={vserverOptions}
                          onValueChange={(val) => setActiveSid(Number(val))}
                        />
                      </div>
                    </>
                  ) : null}
                </div>
              ) : (
                <span className="hidden sm:flex text-sm font-medium text-text-secondary items-center gap-2 truncate">
                  <Activity className="w-4 h-4 text-text-muted shrink-0" aria-hidden="true" />
                  <span className="truncate">{t.appName}</span>
                </span>
              )}
            </div>

            {/* Right Utility Controls */}
            <div className="flex items-center gap-2 shrink-0">
              {wsConnected ? (
                <span
                  className="inline-flex items-center gap-1.5 min-h-7 px-2.5 py-0.5 rounded-full bg-success-subtle border border-success/20 text-success-text text-xs font-medium"
                  title={t.common.wsLive}
                >
                  <span className="relative flex w-2 h-2 shrink-0" aria-hidden="true">
                    <span className="motion-safe:animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-55" />
                    <span className="relative inline-flex rounded-full w-2 h-2 bg-success" />
                  </span>
                  <span className="hidden md:inline tss-trim">{t.common.wsLive}</span>
                </span>
              ) : (
                <StatusBadge
                  tone="warning"
                  label={t.common.wsReconnecting}
                  icon="pending"
                />
              )}

              <button
                type="button"
                onClick={() => setLocale(locale === 'zh-CN' ? 'en-US' : 'zh-CN')}
                className="min-h-8 px-2 sm:px-2.5 inline-flex items-center gap-1.5 text-xs font-medium rounded-[var(--radius-DEFAULT)] border border-border bg-surface-raised text-text-secondary hover:text-text hover:border-border-strong shadow-[var(--shadow-sm)] cursor-pointer transition-colors"
                title="Switch Language"
              >
                <Languages className="w-3.5 h-3.5 text-text-muted hidden sm:inline" aria-hidden="true" />
                <span className="tss-trim">{locale === 'zh-CN' ? 'EN' : '中文'}</span>
              </button>

              {/* Mobile compact theme button (<640px) prevents brand truncation */}
              <button
                type="button"
                onClick={() => {
                  const order: ThemeChoice[] = ['system', 'light', 'dark'];
                  setTheme(order[(order.indexOf(theme) + 1) % order.length]);
                }}
                aria-label="Theme"
                className="sm:hidden w-8 h-8 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] border border-border bg-surface-raised text-text-secondary hover:text-text shadow-[var(--shadow-sm)] cursor-pointer"
              >
                {theme === 'dark' ? (
                  <Moon className="w-3.5 h-3.5" aria-hidden="true" />
                ) : theme === 'light' ? (
                  <Sun className="w-3.5 h-3.5" aria-hidden="true" />
                ) : (
                  <Monitor className="w-3.5 h-3.5" aria-hidden="true" />
                )}
              </button>

              <div
                role="group"
                aria-label="Theme"
                className="hidden sm:inline-flex tss-well p-0.5 items-center gap-0.5"
              >
                {themeOptions.map((opt) => {
                  const Icon = opt.icon;
                  const active = theme === opt.id;
                  return (
                    <button
                      key={opt.id}
                      type="button"
                      aria-pressed={active}
                      aria-label={opt.label}
                      title={opt.label}
                      onClick={() => setTheme(opt.id)}
                      className={`w-7 h-7 inline-flex items-center justify-center rounded-[var(--radius-sm)] cursor-pointer transition-colors ${
                        active
                          ? 'bg-surface-raised text-text border border-border-strong/70 shadow-[var(--shadow-sm)]'
                          : 'text-text-muted hover:text-text border border-transparent'
                      }`}
                    >
                      <Icon className="w-3.5 h-3.5" strokeWidth={1.8} aria-hidden="true" />
                    </button>
                  );
                })}
              </div>
            </div>
          </div>

          {/* Mobile Instance & Virtual Server bar using custom Top-Layer SelectField */}
          {instances.length > 0 ? (
            <div className="sm:hidden px-4 py-2 border-t border-border bg-surface/80 space-y-1.5">
              <div className="flex items-center gap-2 min-w-0">
                <span className="w-14 shrink-0 text-[11px] font-medium text-text-muted truncate">
                  {t.common.selectInstance}
                </span>
                <div className="flex-1 min-w-0">
                  <SelectField
                    id="global-instance-select-mobile"
                    ariaLabel={t.common.selectInstance}
                    size="sm"
                    variant="toolbar"
                    value={activeInstanceId}
                    options={instanceOptions}
                    onValueChange={setActiveInstanceId}
                  />
                </div>
              </div>
              {vservers.length > 0 ? (
                <div className="flex items-center gap-2 min-w-0">
                  <span className="w-14 shrink-0 text-[11px] font-medium text-text-muted truncate">
                    {t.common.selectVServer}
                  </span>
                  <div className="flex-1 min-w-0">
                    <SelectField
                      id="global-vserver-select-mobile"
                      ariaLabel={t.common.selectVServer}
                      size="sm"
                      variant="toolbar"
                      value={String(activeSid)}
                      options={vserverOptions}
                      onValueChange={(val) => setActiveSid(Number(val))}
                    />
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
        </header>

        <main className="flex-1 p-4 sm:p-6 lg:px-8 lg:py-6 max-w-7xl w-full mx-auto">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

export function NoInstanceBanner() {
  const { t } = useI18n();
  return (
    <div className="tss-panel p-6 flex items-center gap-3">
      <Radio className="w-5 h-5 text-text-muted shrink-0" aria-hidden="true" />
      <p className="text-sm text-text-secondary">{t.dashboard.emptyDesc}</p>
    </div>
  );
}
