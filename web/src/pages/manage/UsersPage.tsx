import { useEffect, useState } from 'react'
import {
  api,
  type ManagedUser,
  type ProviderView,
  type SignInSettings,
  type TestReport,
} from '../../api/client'
import { useApi } from '../../api/useApi'
import { Failed, Loading } from '../../components/Status'
import { ago } from '../../lib/format'
import { useUser } from '../../lib/session'
import settings from '../Settings.module.css'
import { ProviderForm } from './ProviderForm'
import { TestReportView } from './ProviderTest'
import { presetOf } from './providerPresets'
import styles from './Users.module.css'

/** Fired after a change to users, so the tab's waiting count follows. */
export const USERS_CHANGED_EVENT = 'mc:users-changed'

function usersChanged() {
  window.dispatchEvent(new Event(USERS_CHANGED_EVENT))
}

/** Runs a change, then `done`; keeps the error to show. */
function useAction(done: () => void) {
  const [error, setError] = useState<string | null>(null)
  const act = async (change: () => Promise<unknown>) => {
    setError(null)
    try {
      await change()
      done()
      usersChanged()
    } catch (e) {
      setError((e as Error).message)
    }
  }
  return { error, act }
}

function signInMethods(user: ManagedUser) {
  const methods = user.logins.map((l) => l.provider_name)
  if (user.has_password) {
    methods.unshift('password')
  }
  return methods.length ? `Signs in with ${methods.join(', ')}` : 'No way to sign in'
}

function Waiting({ user, onChange }: { user: ManagedUser; onChange: () => void }) {
  const { error, act } = useAction(onChange)
  const via = user.logins[0]
  return (
    <li className={settings.item}>
      <span className={settings.itemText}>
        <b>{user.name}</b>
        <span className={settings.muted}>
          {[
            user.email ?? via?.email,
            via && `signed in with ${via.provider_name}`,
            ago(user.created_at),
          ]
            .filter(Boolean)
            .join(' · ')}
        </span>
        {error && <span className={settings.warning}>{error}</span>}
      </span>
      <span className={settings.row}>
        <button
          type="button"
          className={`${settings.button} ${settings.primary}`}
          onClick={() => act(() => api.setUserStatus(user.id, 'active'))}
        >
          Let in
        </button>
        <button
          type="button"
          className={`${settings.button} ${settings.danger}`}
          title="They'll be told their access was declined if they sign in again"
          onClick={() => act(() => api.setUserStatus(user.id, 'disabled'))}
        >
          Decline
        </button>
      </span>
    </li>
  )
}

function UserRow({ user, onChange }: { user: ManagedUser; onChange: () => void }) {
  const me = useUser()
  const { error, act } = useAction(onChange)
  const off = user.status === 'disabled'
  const setPassword = () => {
    const password = window.prompt(
      user.has_password
        ? `New password for ${user.name} (at least 8 characters). They'll be signed out everywhere.`
        : `A password for ${user.name} (at least 8 characters), as well as how they sign in now.`,
    )
    if (password) {
      act(() => api.updateUser(user.id, { password }))
    }
  }
  const setEmail = () => {
    const email = window.prompt(
      `Email address for ${user.name}. Signing in with another account that has this address (verified) offers to link the two. Leave empty to remove it.`,
      user.email ?? '',
    )
    if (email !== null) {
      act(() => api.updateUser(user.id, { email }))
    }
  }
  const role = user.is_admin ? 'Admin' : 'Viewer'
  return (
    <li className={settings.item}>
      <span className={settings.itemText}>
        <b>
          {user.name}
          {user.id === me.id && ' (you)'}
          {off && <span className={styles.badge}>Turned off</span>}
        </b>
        <span className={settings.muted}>
          {user.role_from ? `${role} (from ${user.role_from})` : role}
          {user.email && ` · ${user.email}`} · added {ago(user.created_at)}
        </span>
        <span className={settings.muted}>{signInMethods(user)}</span>
        {user.logins.length > 0 && (
          <span className={styles.logins}>
            {user.logins.map((l) => (
              <span key={l.id} className={styles.login}>
                {l.provider_name}
                {l.email && `: ${l.email}`}
                <button
                  type="button"
                  className={styles.unlink}
                  aria-label={`Unlink ${l.provider_name}`}
                  title="Unlink"
                  onClick={() =>
                    window.confirm(`Unlink ${user.name}'s ${l.provider_name} account?`) &&
                    act(() => api.unlinkUser(user.id, l.id))
                  }
                >
                  ×
                </button>
              </span>
            ))}
          </span>
        )}
        {error && <span className={settings.warning}>{error}</span>}
      </span>
      <span className={settings.row}>
        <button type="button" className={settings.button} onClick={setEmail}>
          Email
        </button>
        <button type="button" className={settings.button} onClick={setPassword}>
          {user.has_password ? 'Reset password' : 'Set password'}
        </button>
        <button
          type="button"
          className={settings.button}
          disabled={!!user.role_from}
          title={
            user.role_from
              ? `Managed by ${user.role_from}: its admin group decides this`
              : undefined
          }
          onClick={() => act(() => api.updateUser(user.id, { is_admin: !user.is_admin }))}
        >
          {user.is_admin ? 'Make viewer' : 'Make admin'}
        </button>
        {user.id !== me.id && !user.is_admin && (
          <button
            type="button"
            className={settings.button}
            title={off ? undefined : 'Signs them out; they keep their watch history'}
            onClick={() => act(() => api.setUserStatus(user.id, off ? 'active' : 'disabled'))}
          >
            {off ? 'Turn on' : 'Turn off'}
          </button>
        )}
        {user.id !== me.id && (
          <button
            type="button"
            className={`${settings.button} ${settings.danger}`}
            onClick={() =>
              window.confirm(`Remove ${user.name} and their watch history?`) &&
              act(() => api.deleteUser(user.id))
            }
          >
            Remove
          </button>
        )}
      </span>
    </li>
  )
}

function Users() {
  const users = useApi('users', api.users)
  const [draft, setDraft] = useState({ name: '', password: '', admin: false })
  const [error, setError] = useState<string | null>(null)
  if (users.error) {
    return <Failed error={users.error} onRetry={users.reload} />
  }
  const create = async () => {
    setError(null)
    try {
      await api.createUser(draft.name, draft.password, draft.admin)
      setDraft({ name: '', password: '', admin: false })
      users.reload()
    } catch (e) {
      setError((e as Error).message)
    }
  }
  const waiting = users.data?.filter((u) => u.status === 'pending') ?? []
  const others = users.data?.filter((u) => u.status !== 'pending') ?? []
  return (
    <>
      {waiting.length > 0 && (
        <section aria-labelledby="waiting" className={settings.panel}>
          <h2 id="waiting">Waiting to be let in</h2>
          <p className={settings.muted}>
            They signed in with another account. Once let in, they can watch everything; you can
            make them an admin afterwards.
          </p>
          <ul className={settings.list}>
            {waiting.map((u) => (
              <Waiting key={u.id} user={u} onChange={users.reload} />
            ))}
          </ul>
        </section>
      )}
      <section aria-labelledby="users" className={settings.panel}>
        <h2 id="users">Users</h2>
        <p className={settings.muted}>
          Everyone gets their own watch history, resume points and playlists. Admins can also manage
          libraries, matching and these settings.
        </p>
        {!users.data && <Loading />}
        {users.data && (
          <ul className={settings.list}>
            {others.map((u) => (
              <UserRow key={u.id} user={u} onChange={users.reload} />
            ))}
          </ul>
        )}
        <div className={settings.row}>
          <input
            className={settings.input}
            aria-label="Name"
            placeholder="Name"
            autoComplete="off"
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          />
          <input
            className={settings.input}
            aria-label="Password"
            placeholder="Password (8+ characters)"
            type="password"
            autoComplete="new-password"
            value={draft.password}
            onChange={(e) => setDraft({ ...draft, password: e.target.value })}
          />
          <label className={settings.check}>
            <input
              type="checkbox"
              checked={draft.admin}
              onChange={(e) => setDraft({ ...draft, admin: e.target.checked })}
            />
            Admin
          </label>
          <button
            type="button"
            className={`${settings.button} ${settings.primary}`}
            disabled={!draft.name.trim() || !draft.password}
            onClick={create}
          >
            Add user
          </button>
        </div>
        {error && <p className={settings.warning}>{error}</p>}
      </section>
    </>
  )
}

function providerState(p: ProviderView) {
  const parts = []
  if (!p.enabled) {
    parts.push('Off')
  }
  if (p.env?.missing) {
    parts.push('its variables are no longer set')
  } else if (p.env?.overridden) {
    parts.push(`edited here instead of ${p.env.prefix}*`)
  } else if (p.env) {
    parts.push(`from ${p.env.prefix}*`)
  }
  parts.push(p.users === 1 ? '1 person linked' : `${p.users} people linked`)
  parts.push(p.new_users === 'active' ? 'new people let in' : 'new people wait for approval')
  return parts.join(' · ')
}

function ProviderRow({
  provider,
  base,
  onChange,
}: {
  provider: ProviderView
  base: string
  onChange: () => void
}) {
  const [open, setOpen] = useState(false)
  const { error, act } = useAction(onChange)
  const p = provider
  const fromEnv = !!p.env && !p.env.overridden
  const remove = () => {
    const stranded =
      p.sole_users > 0
        ? ` ${p.sole_users} ${p.sole_users === 1 ? 'person signs' : 'people sign'} in only with it and will be turned off until you give them a password.`
        : ''
    if (
      window.confirm(
        `Remove ${p.name}? Everyone's linked ${p.name} accounts are unlinked.${stranded}`,
      )
    ) {
      act(() => api.deleteProvider(p.key))
    }
  }
  return (
    <li className={settings.item}>
      <span className={settings.itemText}>
        <b>{p.name}</b>
        <span className={settings.muted}>
          {p.kind === 'oidc' && <code>{p.issuer}</code>}
          {p.kind === 'oauth2' && <code>{p.authorize_url}</code>}
          {(p.kind === 'github' || p.kind === 'discord') && presetOf(p.kind, p.preset).label}
        </span>
        <span className={settings.muted}>{providerState(p)}</span>
        {error && <span className={settings.warning}>{error}</span>}
      </span>
      <span className={settings.row}>
        <button type="button" className={settings.button} onClick={() => setOpen(!open)}>
          {fromEnv ? 'View' : 'Edit'}
        </button>
        {fromEnv && (
          <button
            type="button"
            className={settings.button}
            title={`Copies the settings here; ${p.env?.prefix}* are ignored from then on`}
            onClick={() => act(() => api.overrideProvider(p.key))}
          >
            Edit here instead
          </button>
        )}
        {p.env?.overridden && !p.env.missing && (
          <button
            type="button"
            className={settings.button}
            title={`Discards changes made here and uses ${p.env.prefix}* again`}
            onClick={() => act(() => api.revertProvider(p.key))}
          >
            Go back to the environment
          </button>
        )}
        {(!fromEnv || p.env?.missing) && (
          <button
            type="button"
            className={`${settings.button} ${settings.danger}`}
            onClick={remove}
          >
            Remove
          </button>
        )}
      </span>
      {open && (
        <div className={styles.full}>
          <ProviderForm
            key={`${p.key}:${fromEnv}`}
            provider={p}
            base={base}
            onDone={() => {
              setOpen(false)
              onChange()
            }}
          />
        </div>
      )}
    </li>
  )
}

/** A test sign-in whose window couldn't hand its report back lands here with `?sign_in_test=`. */
function useLandedTest() {
  const [report, setReport] = useState<TestReport | null>(null)
  useEffect(() => {
    const token = new URLSearchParams(window.location.search).get('sign_in_test')
    if (!token) {
      return
    }
    window.history.replaceState(null, '', window.location.pathname)
    api.testReport(token).then(setReport, () => {})
  }, [])
  return [report, () => setReport(null)] as const
}

function SignIn() {
  const data = useApi('sign-in-settings', api.signInSettings)
  const [landed, dismiss] = useLandedTest()
  const [adding, setAdding] = useState(false)
  const { error, act } = useAction(data.reload)
  if (data.error) {
    return <Failed error={data.error} onRetry={data.reload} />
  }
  const s: SignInSettings | null = data.data
  return (
    <section aria-labelledby="sign-in" className={settings.panel}>
      <h2 id="sign-in">Sign-in</h2>
      <p className={settings.muted}>
        Let people sign in with an account they already have: Authentik, Authelia, Keycloak, Pocket
        ID, Google, GitHub, Discord or anything else that speaks OpenID Connect or OAuth2. Providers
        can also come from <code>CABINET_OIDC_*</code> environment variables.
      </p>
      {landed && (
        <div className={styles.field}>
          <span className={styles.label}>Test sign-in with {landed.provider}</span>
          <TestReportView report={landed} />
          <span className={settings.row}>
            <button type="button" className={settings.button} onClick={dismiss}>
              Done
            </button>
          </span>
        </div>
      )}
      {!s && <Loading />}
      {s && (
        <>
          <label
            className={settings.check}
            title={s.password_sign_in_from_env ? 'Set by CABINET_PASSWORD_SIGN_IN' : undefined}
          >
            <input
              type="checkbox"
              checked={s.password_sign_in}
              disabled={s.password_sign_in_from_env}
              onChange={(e) => act(() => api.setPasswordSignIn(e.target.checked))}
            />
            Show password sign-in on the login page
          </label>
          <p className={settings.muted}>
            Off: the login page shows only the providers, with a small “Use a password” link so
            nobody is locked out. With one provider, it goes straight there.
          </p>
          <label
            className={settings.check}
            title={s.guest_access_from_env ? 'Set by CABINET_GUEST_ACCESS' : undefined}
          >
            <input
              type="checkbox"
              checked={s.guest_access}
              disabled={s.guest_access_from_env}
              onChange={(e) => {
                const on = e.target.checked
                if (
                  on ||
                  !s.guests ||
                  window.confirm(
                    `Turn off guests? The ${s.guests === 1 ? 'guest' : `${s.guests} guests`} here now will be signed out and forgotten.`,
                  )
                ) {
                  act(() => api.setGuestAccess(on))
                }
              }}
            />
            Let people continue as a guest
          </label>
          <p className={settings.muted}>
            For a demo server: anyone who can reach this server can watch without an account, and
            apps can offer it too. Each guest is their own, can't change anything but their watch
            history and playlists, and is forgotten when they sign out.
            {s.guest_access &&
              ` ${s.guests === 1 ? '1 guest' : `${s.guests} guests`} now. Link straight in with /?guest.`}
          </p>
          {error && <p className={settings.warning}>{error}</p>}
          {s.providers.length > 0 && (
            <ul className={settings.list}>
              {s.providers.map((p) => (
                <ProviderRow
                  key={p.key}
                  provider={p}
                  base={s.callback_base}
                  onChange={data.reload}
                />
              ))}
            </ul>
          )}
          {adding ? (
            <ProviderForm
              provider={null}
              base={s.callback_base}
              onDone={() => {
                setAdding(false)
                data.reload()
              }}
            />
          ) : (
            <div className={settings.row}>
              <button
                type="button"
                className={`${settings.button} ${settings.primary}`}
                onClick={() => setAdding(true)}
              >
                Add provider
              </button>
            </div>
          )}
        </>
      )}
    </section>
  )
}

/** Who can sign in, and how. */
export function UsersPage() {
  return (
    <div className={settings.page}>
      <div className={settings.inner}>
        <div className={settings.heading}>
          <h1>Users</h1>
          <p className={settings.muted}>Who can sign in, and how.</p>
        </div>
        <Users />
        <SignIn />
      </div>
    </div>
  )
}
