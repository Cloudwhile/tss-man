import React, { useState } from 'react';
import { Eye, EyeOff, KeyRound, Languages, Layers, Moon, Monitor, Radio, ShieldCheck, Sun } from 'lucide-react';
import { ThemeChoice, useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import { apiRequest, AuthUserProfile } from '../lib/api';
import { TssBrandMark } from '../components/TssBrandMark';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { FormField, Input, useFieldId } from '../components/ui/FormControls';

export function SetupWizardPage() {
  const { t, locale, setLocale } = useI18n();
  const { setInitializedAndUser, theme, setTheme } = useApp();

  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [showPw, setShowPw] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const userId = useFieldId('setup-username');
  const displayId = useFieldId('setup-display');
  const pwId = useFieldId('setup-password');

  const cycleTheme = () => {
    const order: ThemeChoice[] = ['system', 'light', 'dark'];
    setTheme(order[(order.indexOf(theme) + 1) % order.length]);
  };
  const ThemeIcon = theme === 'dark' ? Moon : theme === 'light' ? Sun : Monitor;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const resp = await apiRequest<{
        access_token: string;
        user: AuthUserProfile;
      }>('/setup/init', {
        method: 'POST',
        body: JSON.stringify({
          username: username.trim(),
          display_name: displayName.trim() || username.trim(),
          password,
        }),
      });
      await setInitializedAndUser(resp.user, resp.access_token);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Initialization failed');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="min-h-screen bg-surface tss-auth-backdrop text-text flex flex-col justify-center px-4 sm:px-6 py-10">
      <div className="w-full max-w-5xl mx-auto tss-frame overflow-hidden grid grid-cols-1 lg:grid-cols-12">
        {/* Dark Architectural Showcase Column */}
        <section className="lg:col-span-5 tss-gate-showcase p-7 sm:p-8 flex flex-col justify-between gap-8 border-b lg:border-b-0">
          <div className="space-y-6">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <TssBrandMark className="w-10 h-10" />
                <div>
                  <div className="text-base font-semibold text-white leading-tight">{t.appName}</div>
                  <div className="text-xs text-slate-400">{t.appSubtitle}</div>
                </div>
              </div>
              <div className="flex items-center gap-1 lg:hidden">
                <button
                  type="button"
                  onClick={() => setLocale(locale === 'zh-CN' ? 'en-US' : 'zh-CN')}
                  className="min-h-8 px-2 text-xs font-medium rounded-[var(--radius-DEFAULT)] border border-white/15 bg-white/5 text-slate-200 hover:text-white cursor-pointer"
                >
                  {locale === 'zh-CN' ? 'EN' : '中文'}
                </button>
                <button
                  type="button"
                  onClick={cycleTheme}
                  aria-label="Theme"
                  className="min-h-8 w-8 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] border border-white/15 bg-white/5 text-slate-200 hover:text-white cursor-pointer"
                >
                  <ThemeIcon className="w-3.5 h-3.5" aria-hidden="true" />
                </button>
              </div>
            </div>

            <div>
              <div className="font-mono text-[11px] uppercase tracking-wider text-blue-400">
                {t.setup.gateEyebrow}
              </div>
              <h1 className="mt-2 text-2xl font-semibold text-white tracking-tight leading-tight">
                {t.setup.gateHeadline}
              </h1>
            </div>
          </div>

          <ul className="space-y-4 list-none p-0 m-0">
            <li className="flex items-start gap-3">
              <div className="w-8 h-8 rounded-[var(--radius-DEFAULT)] bg-blue-500/15 border border-blue-400/20 text-blue-300 grid place-items-center shrink-0 mt-0.5">
                <Radio className="w-4 h-4" aria-hidden="true" />
              </div>
              <div className="min-w-0">
                <div className="text-sm font-medium text-white">{t.setup.specProtoTitle}</div>
                <p className="text-xs text-slate-400 leading-normal mt-0.5">
                  {t.setup.specProtoDesc}
                </p>
              </div>
            </li>
            <li className="flex items-start gap-3">
              <div className="w-8 h-8 rounded-[var(--radius-DEFAULT)] bg-blue-500/15 border border-blue-400/20 text-blue-300 grid place-items-center shrink-0 mt-0.5">
                <KeyRound className="w-4 h-4" aria-hidden="true" />
              </div>
              <div className="min-w-0">
                <div className="text-sm font-medium text-white">{t.setup.specCryptoTitle}</div>
                <p className="text-xs text-slate-400 leading-normal mt-0.5">
                  {t.setup.specCryptoDesc}
                </p>
              </div>
            </li>
            <li className="flex items-start gap-3">
              <div className="w-8 h-8 rounded-[var(--radius-DEFAULT)] bg-blue-500/15 border border-blue-400/20 text-blue-300 grid place-items-center shrink-0 mt-0.5">
                <Layers className="w-4 h-4" aria-hidden="true" />
              </div>
              <div className="min-w-0">
                <div className="text-sm font-medium text-white">{t.setup.specSyncTitle}</div>
                <p className="text-xs text-slate-400 leading-normal mt-0.5">
                  {t.setup.specSyncDesc}
                </p>
              </div>
            </li>
          </ul>
        </section>

        {/* Form Panel Right Column */}
        <section className="lg:col-span-7 bg-surface-raised p-7 sm:p-9">
          <div className="flex items-start justify-between gap-4 mb-6">
            <div>
              <h2 className="text-lg font-semibold text-text leading-tight">{t.setup.title}</h2>
              <p className="mt-1.5 text-sm text-text-secondary leading-normal tss-prose">
                {t.setup.subtitle}
              </p>
            </div>
            <div className="hidden lg:flex items-center gap-1 shrink-0">
              <button
                type="button"
                onClick={() => setLocale(locale === 'zh-CN' ? 'en-US' : 'zh-CN')}
                className="min-h-8 px-2 inline-flex items-center gap-1 text-xs font-medium rounded-[var(--radius-DEFAULT)] text-text-muted hover:text-text hover:bg-surface cursor-pointer"
              >
                <Languages className="w-3.5 h-3.5" aria-hidden="true" />
                <span>{locale === 'zh-CN' ? 'EN' : '中文'}</span>
              </button>
              <button
                type="button"
                onClick={cycleTheme}
                aria-label="Theme"
                className="min-h-8 w-8 inline-flex items-center justify-center rounded-[var(--radius-DEFAULT)] text-text-muted hover:text-text hover:bg-surface cursor-pointer"
              >
                <ThemeIcon className="w-3.5 h-3.5" aria-hidden="true" />
              </button>
            </div>
          </div>

          {error ? (
            <AlertBanner tone="danger" className="mb-5" onDismiss={() => setError(null)}>
              {error}
            </AlertBanner>
          ) : null}

          <form onSubmit={handleSubmit} method="POST">
            <FormField
              label={t.setup.username}
              htmlFor={userId}
              hint={t.setup.usernameHint}
              required
              className="max-w-none"
            >
              <Input
                id={userId}
                name="username"
                type="text"
                required
                minLength={3}
                maxLength={64}
                autoComplete="username"
                autoCorrect="off"
                autoCapitalize="none"
                spellCheck={false}
                enterKeyHint="next"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
              />
            </FormField>

            <FormField label={t.setup.displayName} htmlFor={displayId} className="max-w-none">
              <Input
                id={displayId}
                name="display_name"
                type="text"
                autoComplete="name"
                enterKeyHint="next"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
              />
            </FormField>

            <FormField
              label={t.setup.password}
              htmlFor={pwId}
              hint={t.setup.passwordHint}
              required
              className="max-w-none"
            >
              <div className="relative">
                <Input
                  id={pwId}
                  name="password"
                  type={showPw ? 'text' : 'password'}
                  required
                  minLength={8}
                  autoComplete="new-password"
                  enterKeyHint="done"
                  className="pe-11"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
                <button
                  type="button"
                  aria-pressed={showPw}
                  aria-label={showPw ? t.login.hidePassword : t.login.showPassword}
                  onClick={() => setShowPw((v) => !v)}
                  className="absolute inset-y-0 right-0 px-3 inline-flex items-center text-text-muted hover:text-text cursor-pointer"
                >
                  {showPw ? (
                    <EyeOff className="w-4 h-4" aria-hidden="true" />
                  ) : (
                    <Eye className="w-4 h-4" aria-hidden="true" />
                  )}
                </button>
              </div>
            </FormField>

            <Button type="submit" variant="primary" className="w-full" disabled={submitting}>
              <ShieldCheck className="w-4 h-4" aria-hidden="true" />
              <span>{submitting ? t.common.loading : t.setup.submit}</span>
            </Button>
          </form>
        </section>
      </div>
    </div>
  );
}
