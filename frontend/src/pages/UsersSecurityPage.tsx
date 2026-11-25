import { useCallback, useEffect, useState } from 'react';
import { KeyRound, Plus } from 'lucide-react';
import { useApp } from '../context/AppContext';
import { useI18n } from '../i18n';
import { apiRequest, AuthUserProfile, InstanceRoleBinding, UserRowView } from '../lib/api';
import { formatRoleLabel } from '../lib/format';
import { AlertBanner } from '../components/ui/AlertBanner';
import { Button } from '../components/ui/Button';
import { CopyButton } from '../components/ui/CopyButton';
import { TableContainer, Td, Th } from '../components/ui/DataTable';
import { FormField, Input, Select } from '../components/ui/FormControls';
import { ModalDialog } from '../components/ui/ModalDialog';
import { StatusBadge } from '../components/ui/StatusBadge';

export function UsersSecurityPage() {
  const { t, locale } = useI18n();
  const { user, instances, setInitializedAndUser } = useApp();

  const [users, setUsers] = useState<UserRowView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // TOTP enrollment modal
  const [totpBundle, setTotpBundle] = useState<{
    secret_base32: string;
    otpauth_uri: string;
    qr_data_uri: string;
  } | null>(null);
  const [totpCode, setTotpCode] = useState('');
  const [showDisableTotp, setShowDisableTotp] = useState(false);
  const [disablePw, setDisablePw] = useState('');

  // Change password
  const [curPw, setCurPw] = useState('');
  const [newPw, setNewPw] = useState('');

  // Create user modal
  const [showCreateUser, setShowCreateUser] = useState(false);
  const [uUsername, setUUsername] = useState('');
  const [uDisplay, setUDisplay] = useState('');
  const [uPassword, setUPassword] = useState('');
  const [uGlobalRole, setUGlobalRole] = useState<'member' | 'super_admin'>('member');

  // Role assignment modal
  const [roleTarget, setRoleTarget] = useState<UserRowView | null>(null);
  const [roleDraft, setRoleDraft] = useState<Record<string, string>>({});

  const isSuper = user?.global_role === 'super_admin';

  const loadUsers = useCallback(async () => {
    if (!isSuper) return;
    try {
      setUsers(await apiRequest<UserRowView[]>('/users'));
    } catch {}
  }, [isSuper]);

  useEffect(() => {
    loadUsers();
  }, [loadUsers]);

  const refreshMe = async () => {
    const me = await apiRequest<AuthUserProfile>('/auth/me');
    await setInitializedAndUser(me);
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl sm:text-2xl font-semibold text-text tracking-tight">{t.security.title}</h1>
        <p className="mt-0.5 text-sm text-text-secondary tss-prose">{t.security.subtitle}</p>
      </div>

      {error ? (
        <AlertBanner tone="danger" onDismiss={() => setError(null)}>
          {error}
        </AlertBanner>
      ) : null}
      {notice ? (
        <AlertBanner tone="success" onDismiss={() => setNotice(null)}>
          {notice}
        </AlertBanner>
      ) : null}

      {/* Editorial Section 1: TOTP Two-Factor Auth */}
      <section className="grid grid-cols-1 lg:grid-cols-12 gap-6 pt-2">
        <div className="lg:col-span-4 space-y-2">
          <h2 className="text-base font-semibold text-text">{t.security.totpTitle}</h2>
          <p className="text-sm text-text-secondary leading-normal tss-prose">
            {t.security.totpDesc}
          </p>
        </div>
        <div className="lg:col-span-8 tss-panel p-6 flex flex-wrap items-center justify-between gap-4">
          <div className="space-y-1.5 min-w-0 flex-1">
            <StatusBadge
              variant="dot"
              tone={user?.totp_enabled ? 'success' : 'neutral'}
              label={user?.totp_enabled ? t.security.totpEnabled : t.security.totpDisabled}
            />
            <p className="text-xs text-text-muted leading-normal tss-prose">
              {t.security.totpNote}
            </p>
          </div>
          <div className="shrink-0">
            {!user?.totp_enabled ? (
              <Button
                variant="primary"
                onClick={async () => {
                  const res = await apiRequest<{
                    secret_base32: string;
                    otpauth_uri: string;
                    qr_data_uri: string;
                  }>('/auth/totp/setup', { method: 'POST' });
                  setTotpCode('');
                  setTotpBundle(res);
                }}
              >
                <KeyRound className="w-4 h-4" aria-hidden="true" />
                <span>{t.security.totpSetupBtn}</span>
              </Button>
            ) : (
              <Button variant="secondary" onClick={() => setShowDisableTotp(true)}>
                {t.security.totpDisableBtn}
              </Button>
            )}
          </div>
        </div>
      </section>

      {/* Editorial Section 2: Password Rotation */}
      <section className="grid grid-cols-1 lg:grid-cols-12 gap-6 pt-8 border-t border-border">
        <div className="lg:col-span-4 space-y-2">
          <h2 className="text-base font-semibold text-text">{t.security.changePasswordTitle}</h2>
          <p className="text-sm text-text-secondary leading-normal tss-prose">
            {t.security.changePasswordDesc}
          </p>
        </div>
        <div className="lg:col-span-8 tss-panel p-6">
          <form
            className="max-w-md"
            onSubmit={async (e) => {
              e.preventDefault();
              setError(null);
              try {
                await apiRequest('/auth/password', {
                  method: 'PUT',
                  body: JSON.stringify({ current_password: curPw, new_password: newPw }),
                });
                setCurPw('');
                setNewPw('');
                setNotice(t.security.passwordUpdated);
              } catch (err) {
                setError(err instanceof Error ? err.message : 'Password change failed');
              }
            }}
          >
            <FormField label={t.security.currentPassword} htmlFor="pw-cur" required>
              <Input
                id="pw-cur"
                type="password"
                required
                autoComplete="current-password"
                value={curPw}
                onChange={(e) => setCurPw(e.target.value)}
              />
            </FormField>
            <FormField label={t.security.newPassword} htmlFor="pw-new" required>
              <Input
                id="pw-new"
                type="password"
                required
                minLength={8}
                autoComplete="new-password"
                value={newPw}
                onChange={(e) => setNewPw(e.target.value)}
              />
            </FormField>
            <Button type="submit" variant="secondary">
              {t.common.save}
            </Button>
          </form>
        </div>
      </section>

      {/* Editorial Section 3: Team RBAC */}
      {isSuper ? (
        <section className="space-y-4 pt-8 border-t border-border">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 className="text-lg font-semibold text-text">{t.security.usersTitle}</h2>
            <Button variant="secondary" onClick={() => setShowCreateUser(true)}>
              <Plus className="w-4 h-4" aria-hidden="true" />
              <span>{t.security.addUser}</span>
            </Button>
          </div>

          <TableContainer label={t.security.usersTitle}>
            <table className="tss-table tss-table-stack">
              <thead>
                <tr>
                  <Th>{t.setup.username}</Th>
                  <Th>{t.security.globalRole}</Th>
                  <Th>2FA</Th>
                  <Th>{t.security.scopedInstances}</Th>
                  <Th>{t.common.status}</Th>
                  <Th numeric>{t.common.actions}</Th>
                </tr>
              </thead>
              <tbody>
                {users.map((u) => {
                  const roleMeta = formatRoleLabel(u.global_role, locale);
                  return (
                  <tr key={u.id}>
                    <Td label={t.setup.username}>
                      <div className="font-semibold text-text">{u.display_name}</div>
                      <div className="font-mono text-xs text-text-muted">@{u.username}</div>
                    </Td>
                    <Td label={t.security.globalRole}>
                      <StatusBadge
                        tone={roleMeta.tone}
                        icon="none"
                        label={roleMeta.label}
                      />
                    </Td>
                    <Td label="2FA">
                      {u.totp_enabled ? (
                        <StatusBadge tone="success" icon="online" label="TOTP" />
                      ) : (
                        <span className="font-mono text-xs text-text-muted">—</span>
                      )}
                    </Td>
                    <Td label={t.security.scopedInstances}>
                      {u.global_role === 'super_admin'
                        ? t.security.allInstances
                        : u.instance_roles.length === 0
                          ? t.common.none
                          : u.instance_roles
                              .map((r) => {
                                const nm =
                                  instances.find((i) => i.id === r.instance_id)?.name ??
                                  r.instance_id.slice(0, 8);
                                return `${nm}: ${formatRoleLabel(r.role, locale).label}`;
                              })
                              .join(' · ')}
                    </Td>
                    <Td label={t.common.status}>
                      <StatusBadge
                        variant="dot"
                        tone={u.is_active ? 'success' : 'danger'}
                        label={u.is_active ? t.common.enabled : t.common.disabled}
                      />
                    </Td>
                    <Td numeric label={t.common.actions}>
                      <div className="inline-flex gap-2 justify-end">
                        {u.global_role !== 'super_admin' ? (
                          <Button
                            size="xs"
                            variant="secondary"
                            onClick={() => {
                              const draft: Record<string, string> = {};
                              u.instance_roles.forEach((r) => {
                                draft[r.instance_id] = r.role;
                              });
                              setRoleDraft(draft);
                              setRoleTarget(u);
                            }}
                          >
                            {t.security.assignRoles}
                          </Button>
                        ) : null}
                        {u.id !== user?.id ? (
                          <Button
                            size="xs"
                            variant="danger-link"
                            onClick={async () => {
                              await apiRequest(`/users/${u.id}`, {
                                method: 'PUT',
                                body: JSON.stringify({ is_active: !u.is_active }),
                              });
                              await loadUsers();
                            }}
                          >
                            {u.is_active ? t.common.disabled : t.common.enabled}
                          </Button>
                        ) : null}
                      </div>
                    </Td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
          </TableContainer>
        </section>
      ) : null}

      {/* TOTP QR Modal with CopyButton */}
      <ModalDialog
        open={totpBundle !== null}
        onClose={() => setTotpBundle(null)}
        title={t.security.totpSetupBtn}
      >
        {totpBundle ? (
          <form
            onSubmit={async (e) => {
              e.preventDefault();
              setError(null);
              try {
                await apiRequest('/auth/totp/enable', {
                  method: 'POST',
                  body: JSON.stringify({
                    secret_base32: totpBundle.secret_base32,
                    code: totpCode.trim(),
                  }),
                });
                setTotpBundle(null);
                await refreshMe();
                setNotice(t.security.totpEnabled);
              } catch (err) {
                setError(err instanceof Error ? err.message : 'Invalid TOTP code');
              }
            }}
            className="space-y-4"
          >
            <div className="flex flex-col items-center p-4 rounded-[var(--radius-DEFAULT)] bg-white border border-border gap-3">
              <img
                src={totpBundle.qr_data_uri}
                alt="TOTP QR Code"
                className="w-44 h-44 object-contain"
              />
              <div className="flex items-center gap-2 max-w-full">
                <code
                  id="totp-secret-val"
                  className="text-xs font-mono text-neutral-800 select-all break-all"
                >
                  {totpBundle.secret_base32}
                </code>
                <CopyButton targetId="totp-secret-val" />
              </div>
            </div>
            <FormField label={t.login.mfaCode} htmlFor="totp-enroll-code" required className="max-w-none">
              <Input
                id="totp-enroll-code"
                type="text"
                inputMode="numeric"
                pattern="\d{6}"
                maxLength={6}
                required
                autoComplete="one-time-code"
                className="font-mono text-center tracking-widest"
                value={totpCode}
                onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
              />
            </FormField>
            <div className="flex justify-end gap-2">
              <Button variant="tertiary" onClick={() => setTotpBundle(null)}>
                {t.common.cancel}
              </Button>
              <Button type="submit" variant="primary">
                {t.common.confirm}
              </Button>
            </div>
          </form>
        ) : null}
      </ModalDialog>

      {/* Disable TOTP Modal */}
      <ModalDialog
        open={showDisableTotp}
        onClose={() => setShowDisableTotp(false)}
        title={t.security.totpDisableBtn}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest('/auth/totp/disable', {
              method: 'POST',
              body: JSON.stringify({ password: disablePw }),
            });
            setShowDisableTotp(false);
            setDisablePw('');
            await refreshMe();
          }}
        >
          <FormField label={t.security.currentPassword} htmlFor="totp-dis-pw" required>
            <Input
              id="totp-dis-pw"
              type="password"
              required
              autoComplete="current-password"
              value={disablePw}
              onChange={(e) => setDisablePw(e.target.value)}
            />
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" autoFocus onClick={() => setShowDisableTotp(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="danger-confirm">
              {t.security.totpDisableBtn}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Create User Modal */}
      <ModalDialog
        open={showCreateUser}
        onClose={() => setShowCreateUser(false)}
        title={t.security.addUser}
      >
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            await apiRequest('/users', {
              method: 'POST',
              body: JSON.stringify({
                username: uUsername.trim(),
                display_name: uDisplay.trim() || uUsername.trim(),
                password: uPassword,
                global_role: uGlobalRole,
              }),
            });
            setShowCreateUser(false);
            setUUsername('');
            setUDisplay('');
            setUPassword('');
            await loadUsers();
          }}
        >
          <FormField label={t.setup.username} htmlFor="u-new-name" required>
            <Input
              id="u-new-name"
              required
              minLength={3}
              value={uUsername}
              onChange={(e) => setUUsername(e.target.value)}
            />
          </FormField>
          <FormField label={t.setup.displayName} htmlFor="u-new-disp">
            <Input
              id="u-new-disp"
              value={uDisplay}
              onChange={(e) => setUDisplay(e.target.value)}
            />
          </FormField>
          <FormField label={t.setup.password} htmlFor="u-new-pw" required>
            <Input
              id="u-new-pw"
              type="password"
              required
              minLength={8}
              value={uPassword}
              onChange={(e) => setUPassword(e.target.value)}
            />
          </FormField>
          <FormField label={t.security.globalRole} htmlFor="u-new-role">
            <Select
              id="u-new-role"
              value={uGlobalRole}
              onChange={(e) => setUGlobalRole(e.target.value as 'member' | 'super_admin')}
            >
              <option value="member">{t.security.roleMemberOpt}</option>
              <option value="super_admin">{t.security.roleSuperOpt}</option>
            </Select>
          </FormField>
          <div className="flex justify-end gap-2">
            <Button variant="tertiary" onClick={() => setShowCreateUser(false)}>
              {t.common.cancel}
            </Button>
            <Button type="submit" variant="primary">
              {t.common.create}
            </Button>
          </div>
        </form>
      </ModalDialog>

      {/* Assign Instance Roles Modal */}
      <ModalDialog
        open={roleTarget !== null}
        onClose={() => setRoleTarget(null)}
        title={`${t.security.assignRoles}: ${roleTarget?.username ?? ''}`}
      >
        <div className="space-y-4">
          {instances.map((inst) => (
            <div key={inst.id} className="flex items-center justify-between gap-4">
              <span className="text-sm font-medium text-text truncate">{inst.name}</span>
              <Select
                className="w-48"
                value={roleDraft[inst.id] || ''}
                onChange={(e) =>
                  setRoleDraft((prev) => ({ ...prev, [inst.id]: e.target.value }))
                }
              >
                <option value="">{t.security.noAccess}</option>
                <option value="auditor">{t.security.roleAuditor}</option>
                <option value="moderator">{t.security.roleModerator}</option>
                <option value="server_admin">{t.security.roleServerAdmin}</option>
              </Select>
            </div>
          ))}
          <div className="flex justify-end gap-2 pt-3 border-t border-border">
            <Button variant="tertiary" onClick={() => setRoleTarget(null)}>
              {t.common.cancel}
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                if (!roleTarget) return;
                const roles: InstanceRoleBinding[] = Object.entries(roleDraft)
                  .filter(([, r]) => r !== '')
                  .map(([instance_id, role]) => ({
                    instance_id,
                    role: role as InstanceRoleBinding['role'],
                  }));
                await apiRequest(`/users/${roleTarget.id}/roles`, {
                  method: 'PUT',
                  body: JSON.stringify({ roles }),
                });
                setRoleTarget(null);
                await loadUsers();
              }}
            >
              {t.common.save}
            </Button>
          </div>
        </div>
      </ModalDialog>
    </div>
  );
}
