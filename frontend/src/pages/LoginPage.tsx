import React, { useState } from 'react';
import { Eye, EyeOff, KeyRound, Languages, LogIn, Monitor, Moon, Radio, ShieldCheck, Sun } from 'lucide-react';
import { ThemeChoice, useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import { apiRequest, AuthUserProfile } from '../lib/api';
import { TssBrandMark } from '../components/TssBrandMark';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { FormField, Input, useFieldId } from '../components/ui/FormControls';

export function LoginPage() {
  const { t, locale, setLocale } = useI18n();
  const { setInitializedAndUser, theme, setTheme } = useApp();

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [showPw, setShowPw] = useState(false);
  const [mfaTicket, setMfaTicket] = useState<string | null>(null);
  const [totpCode, setTotpCode] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const userId = useFieldId('login-username');
  const pwId = useFieldId('login-password');
  const otpId = useFieldId('login-totp');

  const cycleTheme = () => {
    const order: ThemeChoice[] = ['system', 'light', 'dark'];
    setTheme(order[(order.indexOf(theme) + 1) % order.length]);
  };
  const ThemeIcon = theme === 'dark' ? Moon : theme === 'light' ? Sun : Monitor;

  const handlePasswordSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const resp = await apiRequest<{
        mfa_required: boolean;
        mfa_ticket?: string;
        access_token?: string;
        user?: AuthUserProfile;
      }>('/auth/login', {
        method: 'POST',
        body: JSON.stringify({ username: username.trim(), password }),
      });
      if (resp.mfa_required && resp.mfa_ticket) {
        setMfaTicket(resp.mfa_ticket);
      } else if (resp.user && resp.access_token) {
        await setInitializedAndUser(resp.user, resp.access_token);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Sign in failed');
    } finally {
      setSubmitting(false);
    }
  };

  const handleTotpSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!mfaTicket) return;
    setError(null);
    setSubmitting(true);
    try {
      const resp = await apiRequest<{
        access_token: string;
        user: AuthUserProfile;
      }>('/auth/login/totp', {
        method: 'POST',
        body: JSON.stringify({ mfa_ticket: mfaTicket, code: totpCode.trim() }),
      });
      await setInitializedAndUser(resp.user, resp.access_token);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Verification failed');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="min-h-screen bg-surface tss-auth-backdrop text-text flex flex-col justify-center px-4 sm:px-6 py-10">
      <div className="w-full max-w-4xl mx-auto tss-frame overflow-hidden grid grid-cols-1 lg:grid-cols-12">
        {/* Dark Architectural Identity Column */}
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
                {t.login.gateEyebrow}
              </div>
              <h1 className="mt-2 text-2xl font-semibold text-white tracking-tight leading-tight">
                {t.setup.gateHeadline}
              </h1>
            </div>
          </div>

          <div className="space-y-3 text-xs text-slate-300">
            <div className="flex items-center gap-2.5">
              <Radio className="w-4 h-4 text-blue-400 shrink-0" aria-hidden="true" />
              <span>{t.setup.specProtoTitle}</span>
            </div>
            <div className="flex items-center gap-2.5">
              <ShieldCheck className="w-4 h-4 text-blue-400 shrink-0" aria-hidden="true" />
              <span>{t.setup.specCryptoTitle}</span>
            </div>
          </div>
        </section>

        {/* Right Sign-in Panel */}
        <section className="lg:col-span-7 bg-surface-raised p-7 sm:p-9">
          <div className="flex items-start justify-between gap-4 mb-6">
            <div>
              <h2 className="text-lg font-semibold text-text leading-tight">
                {mfaTicket ? t.login.mfaTitle : t.login.title}
              </h2>
              <p className="mt-1.5 text-sm text-text-secondary leading-normal tss-prose">
                {mfaTicket ? t.login.mfaDesc : t.login.subtitle}
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

          {!mfaTicket ? (
            <form onSubmit={handlePasswordSubmit} method="POST">
              <FormField label={t.login.username} htmlFor={userId} required className="max-w-none">
                <Input
                  id={userId}
                  name="username"
                  type="text"
                  required
                  autoComplete="username"
                  autoCorrect="off"
                  autoCapitalize="none"
                  spellCheck={false}
                  enterKeyHint="next"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
              </FormField>

              <FormField label={t.login.password} htmlFor={pwId} required className="max-w-none">
                <div className="relative">
                  <Input
                    id={pwId}
                    name="password"
                    type={showPw ? 'text' : 'password'}
                    required
                    autoComplete="current-password"
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
                <LogIn className="w-4 h-4" aria-hidden="true" />
                <span>{submitting ? t.common.loading : t.login.submit}</span>
              </Button>
            </form>
          ) : (
            <form onSubmit={handleTotpSubmit} method="POST">
              <FormField label={t.login.mfaCode} htmlFor={otpId} required className="max-w-none">
                <Input
                  id={otpId}
                  name="code"
                  type="text"
                  inputMode="numeric"
                  pattern="\d{6}"
                  maxLength={6}
                  required
                  autoComplete="one-time-code"
                  enterKeyHint="done"
                  autoFocus
                  className="font-mono tracking-widest text-center text-lg"
                  value={totpCode}
                  onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
                />
              </FormField>

              <div className="space-y-3">
                <Button type="submit" variant="primary" className="w-full" disabled={submitting}>
                  <KeyRound className="w-4 h-4" aria-hidden="true" />
                  <span>{submitting ? t.common.loading : t.login.mfaSubmit}</span>
                </Button>
                <Button
                  type="button"
                  variant="tertiary"
                  className="w-full"
                  onClick={() => {
                    setMfaTicket(null);
                    setTotpCode('');
                    setError(null);
                  }}
                >
                  {t.login.backToLogin}
                </Button>
              </div>
            </form>
          )}
        </section>
      </div>
    </div>
  );
}
