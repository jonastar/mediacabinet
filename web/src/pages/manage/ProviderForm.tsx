import { useId, useState } from 'react'
import { api, type ProviderInput, type ProviderView } from '../../api/client'
import { Segmented } from '../../components/Segmented'
import settings from '../Settings.module.css'
import { ProviderTest } from './ProviderTest'
import { GROUPS, PRESETS, type Preset, type Step, presetOf } from './providerPresets'
import styles from './Users.module.css'

/** The server's rule for a key made from a name. */
function keyFromName(name: string) {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32)
}

interface Draft {
  /** The entry picked from the list; empty until one is. */
  choice: string
  key: string
  name: string
  issuer: string
  /** The issuer in the provider's own parts, e.g. Keycloak's address and realm. */
  parts: Record<string, string>
  /** A saved issuer that doesn't fit the parts, shown as a plain URL instead. */
  rawIssuer: boolean
  client_id: string
  client_secret: string
  new_users: 'pending' | 'active'
  groups_claim: string
  admin_group: string
  allowed_groups: string
  scopes: string
  redirect_url: string
  enabled: boolean
  authorize_url: string
  token_url: string
  userinfo_url: string
  id_field: string
  name_field: string
  email_field: string
  verified_field: string
  team_id: string
  key_id: string
}

function draftOf(p: ProviderView | null): Draft {
  const preset = p ? presetOf(p.kind, p.preset) : null
  const parts = p && preset?.issuer ? preset.issuer.parse(p.issuer) : null
  return {
    choice: preset?.id ?? '',
    key: p?.key ?? '',
    name: p?.name ?? '',
    issuer: p?.issuer ?? '',
    parts: parts ?? {},
    rawIssuer: !!(p && preset?.issuer && !parts),
    client_id: p?.client_id ?? '',
    client_secret: '',
    new_users: p?.new_users === 'active' ? 'active' : 'pending',
    groups_claim: p?.groups_claim ?? 'groups',
    admin_group: p?.admin_group ?? '',
    allowed_groups: p?.allowed_groups.join(', ') ?? '',
    scopes: p?.scopes ?? '',
    redirect_url: p?.redirect_url ?? '',
    enabled: p?.enabled ?? true,
    authorize_url: p?.authorize_url ?? '',
    token_url: p?.token_url ?? '',
    userinfo_url: p?.userinfo_url ?? '',
    id_field: p?.id_field ?? '',
    name_field: p?.name_field ?? '',
    email_field: p?.email_field ?? '',
    verified_field: p?.verified_field ?? '',
    team_id: p?.team_id ?? '',
    key_id: p?.key_id ?? '',
  }
}

function presetFor(d: Draft): Preset | null {
  return PRESETS.find((p) => p.id === d.choice) ?? null
}

function usesGroups(preset: Preset) {
  return preset.groups && (preset.kind === 'oidc' || preset.kind === 'oauth2')
}

function issuerOf(d: Draft, preset: Preset) {
  if (preset.kind !== 'oidc') {
    return ''
  }
  if (preset.fixedIssuer) {
    return preset.fixedIssuer
  }
  if (preset.issuer && !d.rawIssuer) {
    return preset.issuer.build(d.parts, d.client_id)
  }
  return d.issuer.trim()
}

/** Whether everything the provider needs is filled in. */
function complete(d: Draft, preset: Preset | null) {
  if (!preset || !d.name.trim() || !d.client_id.trim()) {
    return false
  }
  if (preset.kind === 'oidc' && !preset.fixedIssuer) {
    const parts = preset.issuer && !d.rawIssuer ? preset.issuer.parts : null
    if (parts ? parts.some((p) => !d.parts[p.key]?.trim()) : !d.issuer.trim()) {
      return false
    }
  }
  if (preset.kind === 'oauth2') {
    return !!(d.authorize_url.trim() && d.token_url.trim() && d.userinfo_url.trim())
  }
  if (preset.id === 'apple') {
    return !!(d.team_id.trim() && d.key_id.trim())
  }
  return true
}

function inputOf(d: Draft, preset: Preset): ProviderInput {
  const groups = usesGroups(preset)
  const apple = preset.id === 'apple'
  return {
    key: d.key.trim() || null,
    kind: preset.kind,
    preset: preset.preset,
    name: d.name,
    issuer: issuerOf(d, preset),
    client_id: d.client_id,
    client_secret: d.client_secret || null,
    new_users: d.new_users,
    groups_claim: d.groups_claim,
    admin_group: (groups && d.admin_group) || null,
    allowed_groups: groups
      ? d.allowed_groups
          .split(',')
          .map((g) => g.trim())
          .filter(Boolean)
      : [],
    scopes: d.scopes,
    redirect_url: d.redirect_url || null,
    enabled: d.enabled,
    authorize_url: d.authorize_url || null,
    token_url: d.token_url || null,
    userinfo_url: d.userinfo_url || null,
    id_field: d.id_field || null,
    name_field: d.name_field || null,
    email_field: d.email_field || null,
    verified_field: d.verified_field || null,
    team_id: (apple && d.team_id.trim()) || null,
    key_id: (apple && d.key_id.trim()) || null,
  }
}

/** A labelled input; read-only ones from the environment name their variable. */
function Text({
  label,
  hint,
  value,
  onChange,
  envVar,
  readOnly,
  type = 'text',
  placeholder,
  rows,
}: {
  label: string
  hint?: string
  value: string
  onChange: (value: string) => void
  envVar?: string | null
  readOnly: boolean
  type?: string
  placeholder?: string
  /** A text area this many rows high. */
  rows?: number
}) {
  const id = useId()
  const common = {
    id,
    'aria-describedby': hint ? `${id}-hint` : undefined,
    value,
    readOnly,
    placeholder,
    autoComplete: 'off',
  }
  return (
    <div className={styles.field}>
      <label className={styles.label} htmlFor={id}>
        {label}
      </label>
      {rows ? (
        <textarea
          {...common}
          rows={rows}
          spellCheck={false}
          className={`${settings.input} ${styles.control} ${styles.textarea}`}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <input
          {...common}
          type={type}
          className={`${settings.input} ${styles.control}`}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
      {envVar && <code className={styles.env}>{envVar}</code>}
      {hint && (
        <span id={`${id}-hint`} className={styles.hint}>
          {hint}
        </span>
      )}
    </div>
  )
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  if (!window.isSecureContext) {
    return null
  }
  const copy = async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1500)
  }
  return (
    <button type="button" className={settings.button} onClick={copy}>
      {copied ? 'Copied' : 'Copy'}
    </button>
  )
}

/** The callback URL to paste into the provider. */
function RedirectUrl({ url }: { url: string | null }) {
  return (
    <div className={styles.field}>
      <span className={styles.label}>Redirect URL</span>
      {url ? (
        <span className={styles.redirect}>
          <code>{url}</code>
          <CopyButton text={url} />
        </span>
      ) : (
        <span className={styles.redirect}>
          <span className={styles.hint}>Appears once the provider has a name.</span>
        </span>
      )}
    </div>
  )
}

/** Text with `code` and **bold** parts. */
function Rich({ text }: { text: string }) {
  return (
    <>
      {text.split(/(`[^`]*`|\*\*[^*]+\*\*)/).map((part, i) => {
        if (part.startsWith('`') && part.endsWith('`') && part.length > 1) {
          return <code key={i}>{part.slice(1, -1)}</code>
        }
        if (part.startsWith('**') && part.endsWith('**')) {
          return <b key={i}>{part.slice(2, -2)}</b>
        }
        return part
      })}
    </>
  )
}

/** How to set the provider up at its end, with the redirect URL in reach. */
function Setup({
  preset,
  redirect,
  origin,
  open,
}: {
  preset: Preset
  redirect: string | null
  origin: string
  open: boolean
}) {
  const fill = (text: string) =>
    text.replaceAll('{redirect}', redirect ?? '…/callback').replaceAll('{origin}', origin)
  // A code block belongs to the step before it.
  const steps: { text: string; code: string[] }[] = []
  const all: Step[] = [...preset.steps, ...(usesGroups(preset) ? (preset.groupSteps ?? []) : [])]
  for (const step of all) {
    if (typeof step === 'string') {
      steps.push({ text: step, code: [] })
    } else if (steps.length) {
      steps[steps.length - 1].code.push(step.code)
    }
  }
  return (
    <details className={styles.setup} open={open}>
      <summary>How to set up {preset.label}</summary>
      <div className={styles.setupBody}>
        <RedirectUrl url={redirect} />
        <ol className={styles.steps}>
          {steps.map((s, i) => (
            <li key={i}>
              <Rich text={fill(s.text)} />
              {s.code.map((code, j) => (
                <div key={j} className={styles.codeBlock}>
                  <pre>{fill(code)}</pre>
                  <CopyButton text={fill(code)} />
                </div>
              ))}
            </li>
          ))}
        </ol>
        {preset.notes?.map((n, i) => (
          <p key={i} className={styles.hint}>
            <Rich text={n} />
          </p>
        ))}
        <a className={styles.docs} href={preset.docs} target="_blank" rel="noreferrer">
          {preset.preset ? `${preset.label}'s documentation` : 'More about this'}
        </a>
      </div>
    </details>
  )
}

export function ProviderForm({
  provider,
  base,
  onDone,
}: {
  provider: ProviderView | null
  base: string
  onDone: () => void
}) {
  const [d, setD] = useState(() => draftOf(provider))
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const pickId = useId()
  const readOnly = !!provider?.env && !provider.env.overridden
  const preset = presetFor(d)
  const set =
    <K extends keyof Draft>(k: K) =>
    (v: Draft[K]) =>
      setD((old) => ({ ...old, [k]: v }))
  const setPart = (k: string) => (v: string) =>
    setD((old) => ({ ...old, parts: { ...old.parts, [k]: v } }))
  const envVar = (field: string) =>
    readOnly && provider?.env?.fields.includes(field)
      ? `${provider.env.prefix}${field === 'kind' ? 'TYPE' : field.toUpperCase()}`
      : null
  const key = d.key.trim() || keyFromName(d.name)
  const redirect = d.redirect_url.trim() || (key ? `${base}${key}/callback` : null)
  const origin = URL.canParse(base) ? new URL(base).origin : base
  const ready = complete(d, preset)

  // A new choice brings its name, scopes and groups claim, unless they were changed.
  const choose = (id: string) =>
    setD((old) => {
      const from = presetFor(old)
      const to = PRESETS.find((p) => p.id === id)!
      const typed = old.name.trim() && old.name.trim() !== from?.label
      const claim = (p: Preset | null) => p?.groupsClaim ?? 'groups'
      return {
        ...old,
        choice: id,
        name: typed ? old.name : to.preset ? to.label : '',
        scopes: !from || old.scopes === from.scopes ? to.scopes : old.scopes,
        groups_claim: old.groups_claim === claim(from) ? claim(to) : old.groups_claim,
        parts: Object.fromEntries(
          (to.issuer?.parts ?? []).filter((p) => p.initial).map((p) => [p.key, p.initial!]),
        ),
        rawIssuer: false,
      }
    })

  // Check settings found the issuer under the name the provider gives it.
  const correctIssuer = (issuer: string) =>
    setD((old) => {
      const p = presetFor(old)
      if (!p || p.fixedIssuer) {
        return old
      }
      if (p.issuer && !old.rawIssuer) {
        const parts = p.issuer.parse(issuer)
        return parts ? { ...old, parts: { ...old.parts, ...parts } } : old
      }
      return { ...old, issuer }
    })

  const save = async () => {
    if (!preset) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      const input = inputOf(d, preset)
      if (preset.kind === 'oidc') {
        // Check the issuer first, so a typo fails here and not at someone's sign-in.
        input.issuer = (await api.testIssuer(issuerOf(d, preset))).issuer
      }
      if (provider) {
        await api.updateProvider(provider.key, input)
      } else {
        await api.createProvider(input)
      }
      onDone()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const secretPlaceholder = provider?.has_secret ? (readOnly ? 'Set' : 'Unchanged') : ''

  return (
    <div className={styles.form}>
      {!provider && <h3 className={styles.formTitle}>Add a provider</h3>}
      {readOnly && (
        <p className={styles.hint}>
          Set by <code>{provider?.env?.prefix}*</code> environment variables. To change it here, use
          “Edit here instead”.
        </p>
      )}
      <div className={styles.field}>
        <label className={styles.label} htmlFor={pickId}>
          Provider
        </label>
        {provider ? (
          <span id={pickId}>{preset?.label}</span>
        ) : (
          <select
            id={pickId}
            className={`${settings.input} ${styles.control}`}
            value={d.choice}
            onChange={(e) => choose(e.target.value)}
          >
            {!d.choice && <option value="">Choose where people have their account…</option>}
            {GROUPS.map((g) => (
              <optgroup key={g} label={g}>
                {PRESETS.filter((p) => p.group === g).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        )}
        {envVar('kind') && <code className={styles.env}>{envVar('kind')}</code>}
        {preset?.description && <span className={styles.hint}>{preset.description}</span>}
      </div>

      {preset && (
        <>
          <Setup preset={preset} redirect={redirect} origin={origin} open={!provider} />

          {preset.kind === 'oidc' &&
            !preset.fixedIssuer &&
            (preset.issuer && !d.rawIssuer ? (
              <div className={styles.pair}>
                {preset.issuer.parts.map((part) => (
                  <Text
                    key={part.key}
                    label={part.label}
                    hint={part.hint}
                    value={d.parts[part.key] ?? ''}
                    onChange={setPart(part.key)}
                    envVar={envVar('issuer')}
                    readOnly={readOnly}
                    placeholder={part.placeholder}
                  />
                ))}
              </div>
            ) : (
              <Text
                label="Issuer URL"
                hint="The provider's address for this client; it must use HTTPS."
                value={d.issuer}
                onChange={set('issuer')}
                envVar={envVar('issuer')}
                readOnly={readOnly}
                placeholder="https://auth.example.com/application/o/media-cabinet/"
              />
            ))}
          {preset.kind === 'oauth2' && (
            <>
              <Text
                label="Authorization URL"
                hint="Where people are sent to sign in."
                value={d.authorize_url}
                onChange={set('authorize_url')}
                envVar={envVar('authorize_url')}
                readOnly={readOnly}
                placeholder="https://forum.example.com/oauth/authorize"
              />
              <div className={styles.pair}>
                <Text
                  label="Token URL"
                  hint="Where Media Cabinet trades the code for a token."
                  value={d.token_url}
                  onChange={set('token_url')}
                  envVar={envVar('token_url')}
                  readOnly={readOnly}
                  placeholder="https://forum.example.com/oauth/token"
                />
                <Text
                  label="User URL"
                  hint="Answers who signed in, as JSON."
                  value={d.userinfo_url}
                  onChange={set('userinfo_url')}
                  envVar={envVar('userinfo_url')}
                  readOnly={readOnly}
                  placeholder="https://forum.example.com/api/me"
                />
              </div>
            </>
          )}

          {preset.id === 'apple' ? (
            <>
              <div className={styles.pair}>
                <Text
                  label="Services ID"
                  value={d.client_id}
                  onChange={set('client_id')}
                  envVar={envVar('client_id')}
                  readOnly={readOnly}
                  placeholder="com.example.media"
                />
                <Text
                  label="Team ID"
                  value={d.team_id}
                  onChange={set('team_id')}
                  envVar={envVar('team_id')}
                  readOnly={readOnly}
                />
                <Text
                  label="Key ID"
                  value={d.key_id}
                  onChange={set('key_id')}
                  envVar={envVar('key_id')}
                  readOnly={readOnly}
                />
              </div>
              <Text
                label={preset.secretLabel ?? 'Private key'}
                hint={preset.secretHint}
                value={d.client_secret}
                onChange={set('client_secret')}
                envVar={envVar('client_secret')}
                readOnly={readOnly}
                placeholder={secretPlaceholder || '-----BEGIN PRIVATE KEY-----'}
                rows={5}
              />
            </>
          ) : (
            <div className={styles.pair}>
              <Text
                label={preset.clientIdLabel ?? 'Client ID'}
                value={d.client_id}
                onChange={set('client_id')}
                envVar={envVar('client_id')}
                readOnly={readOnly}
              />
              <Text
                label={preset.secretLabel ?? 'Client secret'}
                hint={preset.secretHint}
                type="password"
                value={d.client_secret}
                onChange={set('client_secret')}
                envVar={envVar('client_secret')}
                readOnly={readOnly}
                placeholder={secretPlaceholder}
              />
            </div>
          )}

          <Text
            label="Name"
            hint="On the login button: “Sign in with …”"
            value={d.name}
            onChange={set('name')}
            envVar={envVar('name')}
            readOnly={readOnly}
            placeholder={preset.preset ? preset.label : 'Authentik'}
          />
          <div className={styles.field}>
            <span className={styles.label}>New people</span>
            {readOnly ? (
              <span>
                {d.new_users === 'active'
                  ? 'Can watch right away'
                  : 'Wait for an admin to let them in'}
              </span>
            ) : (
              <Segmented
                label="New people"
                variant="pills"
                value={d.new_users}
                onChange={set('new_users')}
                options={[
                  { value: 'pending', label: 'Wait for approval' },
                  { value: 'active', label: 'Can watch right away' },
                ]}
              />
            )}
            {envVar('new_users') && <code className={styles.env}>{envVar('new_users')}</code>}
            <span className={styles.hint}>
              For people signing in for the first time. Anyone whose email matches an existing user
              is asked for that user's password instead.
            </span>
          </div>

          <Advanced
            d={d}
            preset={preset}
            set={set}
            envVar={envVar}
            readOnly={readOnly}
            keyFixed={readOnly || !!provider?.env || (provider?.users ?? 0) > 0}
          />

          <ProviderTest
            existing={provider?.key ?? null}
            input={() => inputOf(d, preset)}
            ready={ready}
            onIssuer={readOnly ? () => {} : correctIssuer}
          />
        </>
      )}

      {error && <p className={settings.warning}>{error}</p>}
      <div className={styles.footer}>
        <label className={settings.check}>
          <input
            type="checkbox"
            checked={d.enabled}
            disabled={readOnly}
            onChange={(e) => set('enabled')(e.target.checked)}
          />
          Enabled
        </label>
        <span className={settings.row}>
          <button type="button" className={settings.button} onClick={onDone}>
            {readOnly ? 'Close' : 'Cancel'}
          </button>
          {!readOnly && (
            <button
              type="button"
              className={`${settings.button} ${settings.primary}`}
              disabled={busy || !ready}
              onClick={save}
            >
              {busy ? 'Saving…' : provider ? 'Save' : 'Add provider'}
            </button>
          )}
        </span>
      </div>
    </div>
  )
}

function Advanced({
  d,
  preset,
  set,
  envVar,
  readOnly,
  keyFixed,
}: {
  d: Draft
  preset: Preset
  set: <K extends keyof Draft>(k: K) => (v: Draft[K]) => void
  envVar: (field: string) => string | null
  readOnly: boolean
  keyFixed: boolean
}) {
  return (
    <details className={styles.advanced}>
      <summary>Advanced</summary>
      <div className={styles.advancedBody}>
        {preset.kind === 'oauth2' && (
          <div className={styles.group}>
            <span className={styles.groupTitle}>User fields</span>
            <span className={styles.hint}>
              Where the user URL's answer says who someone is, as dotted paths like user.id. Try
              signing in lists the fields it has.
            </span>
            <div className={styles.pair}>
              <Text
                label="ID field"
                value={d.id_field}
                onChange={set('id_field')}
                envVar={envVar('id_field')}
                readOnly={readOnly}
                placeholder="id"
              />
              <Text
                label="Name field"
                value={d.name_field}
                onChange={set('name_field')}
                envVar={envVar('name_field')}
                readOnly={readOnly}
                placeholder="login or username"
              />
              <Text
                label="Email field"
                value={d.email_field}
                onChange={set('email_field')}
                envVar={envVar('email_field')}
                readOnly={readOnly}
                placeholder="email"
              />
              <Text
                label="Verified field"
                hint="True when the email is verified. Empty: emails aren't trusted to match people."
                value={d.verified_field}
                onChange={set('verified_field')}
                envVar={envVar('verified_field')}
                readOnly={readOnly}
              />
            </div>
          </div>
        )}
        {usesGroups(preset) && (
          <div className={styles.group}>
            <span className={styles.groupTitle}>Groups</span>
            <span className={styles.hint}>
              Only if the provider sends groups. Leave empty to manage roles here.
              {preset.groupsHint && (
                <>
                  {' '}
                  <Rich text={preset.groupsHint} />
                </>
              )}
            </span>
            <div className={styles.pair}>
              <Text
                label="Admin group"
                hint="Members are admins and others aren't, checked at each sign-in."
                value={d.admin_group}
                onChange={set('admin_group')}
                envVar={envVar('admin_group')}
                readOnly={readOnly}
              />
              <Text
                label="Allowed groups"
                hint="Comma-separated. Only members can sign in, and they're let in straight away."
                value={d.allowed_groups}
                onChange={set('allowed_groups')}
                envVar={envVar('allowed_groups')}
                readOnly={readOnly}
              />
              <Text
                label={preset.kind === 'oauth2' ? 'Groups field' : 'Groups claim'}
                hint="Where the provider lists someone's groups."
                value={d.groups_claim}
                onChange={set('groups_claim')}
                envVar={envVar('groups_claim')}
                readOnly={readOnly}
              />
            </div>
          </div>
        )}
        <div className={styles.group}>
          <span className={styles.groupTitle}>Connection</span>
          <div className={styles.pair}>
            <Text
              label="Scopes"
              hint={
                preset.kind === 'oidc'
                  ? "Space-separated. “groups” is added when the groups above need it; Check settings shows what's asked for."
                  : 'Space-separated.'
              }
              value={d.scopes}
              onChange={set('scopes')}
              envVar={envVar('scopes')}
              readOnly={readOnly}
              placeholder={preset.scopes || 'none'}
            />
            <Text
              label="Redirect URL override"
              hint="Only if the one above is wrong, e.g. behind an unusual proxy."
              value={d.redirect_url}
              onChange={set('redirect_url')}
              envVar={envVar('redirect_url')}
              readOnly={readOnly}
            />
            <Text
              label="Key"
              hint="In the redirect URL and env variables. Fixed once someone has signed in."
              value={d.key}
              onChange={set('key')}
              readOnly={keyFixed}
              placeholder={keyFromName(d.name)}
            />
          </div>
        </div>
      </div>
    </details>
  )
}
