import type { ProviderKind } from '../../api/client'

/**
 * Sign-in providers the form knows by name: what to ask for, in the provider's own terms, and how
 * to set it up there. The server knows the same keys (`crates/core/src/sign_in/presets.rs`) for the
 * parts it fills in itself. Steps can use `{redirect}` and `{origin}`, `backticks` and **bold**.
 */

/** A piece of the issuer URL, asked for on its own. */
export interface Part {
  key: string
  label: string
  placeholder: string
  hint?: string
  /** Filled in for a new provider. */
  initial?: string
}

export interface IssuerParts {
  parts: Part[]
  build: (parts: Record<string, string>, clientId: string) => string
  /** The parts of a saved issuer; null when it doesn't fit (then it's shown as a plain URL). */
  parse: (issuer: string) => Record<string, string> | null
}

export type Step = string | { code: string }

export interface Preset {
  /** `other-oidc` and `other-oauth2` aren't stored as presets. */
  id: string
  label: string
  group: 'Self-hosted' | 'Online accounts' | 'Other'
  kind: ProviderKind
  /** Stored with the provider; null for the "Other" entries. */
  preset: string | null
  /** Shown under the choice, for the "Other" entries. */
  description?: string
  /** Set by the provider, not asked for. */
  fixedIssuer?: string
  issuer?: IssuerParts
  scopes: string
  /** Whether the provider can send groups at all. */
  groups: boolean
  groupsClaim?: string
  /** What group names look like, under the group fields. */
  groupsHint?: string
  clientIdLabel?: string
  secretLabel?: string
  secretHint?: string
  steps: Step[]
  /** Optional steps for groups, shown after the others when the provider has them. */
  groupSteps?: Step[]
  notes?: string[]
  docs: string
}

const OIDC_SCOPES = 'openid profile email'

const trim = (url: string) => url.trim().replace(/\/+$/, '')

/** An issuer that is just the server's address. */
function address(label: string, placeholder: string, initial?: string): IssuerParts {
  return {
    parts: [{ key: 'base', label, placeholder, initial }],
    build: (p) => trim(p.base ?? ''),
    parse: (issuer) => ({ base: issuer }),
  }
}

export const PRESETS: Preset[] = [
  {
    id: 'authentik',
    label: 'Authentik',
    group: 'Self-hosted',
    kind: 'oidc',
    preset: 'authentik',
    issuer: {
      parts: [
        { key: 'base', label: 'Authentik address', placeholder: 'https://auth.example.com' },
        {
          key: 'slug',
          label: 'Application slug',
          placeholder: 'media-cabinet',
          hint: "Shown on the application's page in Authentik.",
        },
      ],
      build: (p) => `${trim(p.base ?? '')}/application/o/${(p.slug ?? '').trim()}/`,
      parse: (issuer) => {
        const m = issuer.match(/^(.*)\/application\/o\/([^/]+)\/?$/)
        return m ? { base: m[1], slug: m[2] } : null
      },
    },
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: 'Authentik sends group names.',
    steps: [
      "In Authentik's admin interface, go to **Applications → Applications** and choose **Create with provider**.",
      'Name the application, e.g. Media Cabinet, and note its slug.',
      'Pick **OAuth2/OpenID Provider**, keep the client type **Confidential**, and add `{redirect}` as a **Strict** redirect URI.',
      'Copy the **Client ID** and **Client Secret** into the fields below.',
    ],
    docs: 'https://docs.goauthentik.io/add-secure-apps/providers/oauth2/',
  },
  {
    id: 'authelia',
    label: 'Authelia',
    group: 'Self-hosted',
    kind: 'oidc',
    preset: 'authelia',
    issuer: address('Authelia address', 'https://auth.example.com'),
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: 'Authelia sends group names.',
    steps: [
      'Make a client secret. This prints the secret to paste below and the digest for the configuration:',
      {
        code: 'authelia crypto hash generate pbkdf2 --variant sha512 --random --random.length 72 --random.charset rfc3986',
      },
      "Add the client to Authelia's configuration, with the digest as its `client_secret`, and restart Authelia:",
      {
        code: `identity_providers:
  oidc:
    clients:
      - client_id: 'media-cabinet'
        client_name: 'Media Cabinet'
        client_secret: '$pbkdf2-sha512$…'
        public: false
        authorization_policy: 'one_factor'
        require_pkce: true
        pkce_challenge_method: 'S256'
        redirect_uris:
          - '{redirect}'
        scopes: ['openid', 'profile', 'email', 'groups']
        token_endpoint_auth_method: 'client_secret_basic'`,
      },
      'Enter `media-cabinet` as the client ID below, and the secret (not the digest).',
    ],
    docs: 'https://www.authelia.com/configuration/identity-providers/openid-connect/clients/',
  },
  {
    id: 'keycloak',
    label: 'Keycloak',
    group: 'Self-hosted',
    kind: 'oidc',
    preset: 'keycloak',
    issuer: {
      parts: [
        { key: 'base', label: 'Keycloak address', placeholder: 'https://keycloak.example.com' },
        { key: 'realm', label: 'Realm', placeholder: 'home' },
      ],
      build: (p) => `${trim(p.base ?? '')}/realms/${(p.realm ?? '').trim()}`,
      parse: (issuer) => {
        const m = issuer.match(/^(.*)\/realms\/([^/]+)\/?$/)
        return m ? { base: m[1], realm: m[2] } : null
      },
    },
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: 'Keycloak sends group names once the mapper below is set up.',
    steps: [
      'In your realm, go to **Clients → Create client**. Keep **OpenID Connect** and enter `media-cabinet` as the client ID.',
      'Turn on **Client authentication** and keep **Standard flow**.',
      'Add `{redirect}` under **Valid redirect URIs** and save.',
      "Copy the secret from the client's **Credentials** tab.",
    ],
    groupSteps: [
      'For groups: under **Client scopes → Create client scope**, make one called `groups`. Add a mapper **By configuration → Group Membership** with the token claim name `groups` and **Full group path** off. Then add the scope to the client as **Default**.',
    ],
    docs: 'https://www.keycloak.org/docs/latest/server_admin/index.html#_oidc_clients',
  },
  {
    id: 'pocket-id',
    label: 'Pocket ID',
    group: 'Self-hosted',
    kind: 'oidc',
    preset: 'pocket-id',
    issuer: address('Pocket ID address', 'https://id.example.com'),
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: 'Pocket ID sends group names, not their friendly names.',
    steps: [
      'In Pocket ID, go to **Administration → OIDC Clients** and add a client.',
      'Name it, add `{redirect}` as a **Callback URL**, and save.',
      'Copy the **Client ID** and **Client secret**. The secret is only shown once.',
    ],
    docs: 'https://pocket-id.org/docs/',
  },
  {
    id: 'kanidm',
    label: 'Kanidm',
    group: 'Self-hosted',
    kind: 'oidc',
    preset: 'kanidm',
    issuer: {
      parts: [{ key: 'base', label: 'Kanidm address', placeholder: 'https://idm.example.com' }],
      build: (p, clientId) => `${trim(p.base ?? '')}/oauth2/openid/${clientId.trim()}`,
      parse: (issuer) => {
        const m = issuer.match(/^(.*)\/oauth2\/openid\/[^/]+\/?$/)
        return m ? { base: m[1] } : null
      },
    },
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: 'Kanidm names groups with their domain, like `media_admins@idm.example.com`.',
    steps: [
      'Run these as a Kanidm admin, with the group allowed to sign in in place of `media_users`:',
      {
        code: `kanidm system oauth2 create media-cabinet "Media Cabinet" {origin}
kanidm system oauth2 add-redirect-url media-cabinet {redirect}
kanidm system oauth2 update-scope-map media-cabinet media_users openid profile email groups
kanidm system oauth2 prefer-short-username media-cabinet
kanidm system oauth2 show-basic-secret media-cabinet`,
      },
      'Enter `media-cabinet` as the client ID below, and the secret the last command printed.',
    ],
    docs: 'https://kanidm.github.io/kanidm/stable/integrations/oauth2.html',
  },
  {
    id: 'zitadel',
    label: 'Zitadel',
    group: 'Self-hosted',
    kind: 'oidc',
    preset: 'zitadel',
    issuer: address('Zitadel address', 'https://example.zitadel.cloud'),
    scopes: OIDC_SCOPES,
    groups: true,
    groupsClaim: 'urn:zitadel:iam:org:project:roles',
    groupsHint: "Zitadel sends the project's role keys.",
    steps: [
      'In the Zitadel console, open a project (or create one) and add a **New** application of type **Web**.',
      'Choose **Code** and the authentication method **Basic**, add `{redirect}` as a redirect URI, and create it.',
      'Copy the **Client ID** and **Client Secret**.',
    ],
    groupSteps: [
      "For roles as groups: turn on **Assert Roles on Authentication** in the project, and **User roles inside ID Token** in the application's token settings.",
    ],
    docs: 'https://zitadel.com/docs/guides/integrate/login/oidc',
  },
  {
    id: 'google',
    label: 'Google',
    group: 'Online accounts',
    kind: 'oidc',
    preset: 'google',
    fixedIssuer: 'https://accounts.google.com',
    scopes: OIDC_SCOPES,
    groups: false,
    steps: [
      'In the Google Cloud console, open **Google Auth Platform → Clients** and choose **Create client**.',
      'Choose **Web application**, add `{redirect}` under **Authorized redirect URIs**, and create it.',
      'Copy the **Client ID** and **Client secret**.',
      'Under **Audience**, add people as test users or publish the app, so they can sign in.',
    ],
    docs: 'https://developers.google.com/identity/openid-connect/openid-connect',
  },
  {
    id: 'microsoft',
    label: 'Microsoft',
    group: 'Online accounts',
    kind: 'oidc',
    preset: 'microsoft',
    issuer: {
      parts: [
        {
          key: 'tenant',
          label: 'Directory (tenant) ID',
          placeholder: '00000000-0000-0000-0000-000000000000',
          hint: 'For personal Microsoft accounts only, use 9188040d-6c67-4c5b-b112-36a304b66dad.',
        },
      ],
      build: (p) => `https://login.microsoftonline.com/${(p.tenant ?? '').trim()}/v2.0`,
      parse: (issuer) => {
        const m = issuer.match(/^https:\/\/login\.microsoftonline\.com\/([^/]+)\/v2\.0\/?$/)
        return m ? { tenant: m[1] } : null
      },
    },
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: "Microsoft sends group IDs: use the group's Object ID.",
    clientIdLabel: 'Application (client) ID',
    secretLabel: 'Client secret value',
    secretHint: 'The Value, not the Secret ID.',
    steps: [
      'In the Microsoft Entra admin center, go to **Entra ID → App registrations → New registration**.',
      'Choose who may sign in, and add `{redirect}` as a **Web** redirect URI.',
      'From **Overview**, copy the **Application (client) ID** and **Directory (tenant) ID**.',
      'Under **Certificates & secrets → New client secret**, make one and copy its **Value**.',
    ],
    groupSteps: ['For groups: under **Token configuration**, choose **Add groups claim**.'],
    notes: [
      "Microsoft doesn't say whether emails are verified, so people aren't matched to existing users by email; they link their Microsoft account from Account instead.",
      "One app for both work and personal accounts (the `common` tenant) isn't supported.",
    ],
    docs: 'https://learn.microsoft.com/en-us/entra/identity-platform/quickstart-register-app',
  },
  {
    id: 'gitlab',
    label: 'GitLab',
    group: 'Online accounts',
    kind: 'oidc',
    preset: 'gitlab',
    issuer: address('GitLab address', 'https://gitlab.com', 'https://gitlab.com'),
    scopes: OIDC_SCOPES,
    groups: true,
    groupsHint: 'GitLab sends group paths, like `family/media`.',
    clientIdLabel: 'Application ID',
    steps: [
      "In GitLab, go to **User settings → Applications** (or a group's or the admin area's **Applications**) and add a new application.",
      'Add `{redirect}` as the **Redirect URI**, keep **Confidential** checked, and select the scopes `openid`, `profile` and `email`.',
      'Save, then copy the **Application ID** and **Secret**.',
    ],
    notes: ['GitLab only sends an email that people have made public on their profile.'],
    docs: 'https://docs.gitlab.com/integration/oauth_provider/',
  },
  {
    id: 'github',
    label: 'GitHub',
    group: 'Online accounts',
    kind: 'github',
    preset: 'github',
    scopes: 'read:user user:email',
    groups: false,
    steps: [
      'On GitHub, go to **Settings → Developer settings → OAuth Apps → New OAuth App**.',
      'Enter `{origin}` as the **Homepage URL** and `{redirect}` as the **Authorization callback URL**, then register it.',
      'Copy the **Client ID**, then choose **Generate a new client secret** and copy it.',
    ],
    notes: ["Only someone's primary email is used, and only if GitHub has verified it."],
    docs: 'https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/creating-an-oauth-app',
  },
  {
    id: 'discord',
    label: 'Discord',
    group: 'Online accounts',
    kind: 'discord',
    preset: 'discord',
    scopes: 'identify email',
    groups: false,
    steps: [
      'At discord.com/developers/applications, choose **New Application**.',
      'Under **OAuth2**, add `{redirect}` to **Redirects** and save.',
      'Copy the **Client ID**, and choose **Reset Secret** to get the client secret.',
    ],
    docs: 'https://discord.com/developers/docs/topics/oauth2',
  },
  {
    id: 'apple',
    label: 'Apple',
    group: 'Online accounts',
    kind: 'oidc',
    preset: 'apple',
    fixedIssuer: 'https://appleid.apple.com',
    scopes: 'openid email name',
    groups: false,
    clientIdLabel: 'Services ID',
    secretLabel: 'Private key',
    secretHint: 'The whole .p8 file, including the BEGIN and END lines.',
    steps: [
      'You need a paid Apple Developer account, and an App ID with **Sign in with Apple** turned on.',
      'In **Certificates, Identifiers & Profiles → Identifiers**, add a **Services ID**, e.g. `com.example.media`. That is the client ID.',
      'Turn on **Sign in with Apple** for it and choose **Configure**: pick the App ID, add your domain, and add `{redirect}` as a **Return URL**.',
      'Under **Keys**, add a key with **Sign in with Apple**, download its `.p8` file (you can only do that once) and note its **Key ID**.',
      'Your **Team ID** is under **Membership details**.',
    ],
    notes: [
      'Apple needs a real domain over HTTPS; it won’t accept localhost or an IP address.',
      "Apple only sends someone's name the first time they sign in, and their email may be a private relay address.",
    ],
    docs: 'https://developer.apple.com/help/account/configure-app-capabilities/configure-sign-in-with-apple-for-the-web',
  },
  {
    id: 'other-oidc',
    label: 'Other OpenID Connect',
    group: 'Other',
    kind: 'oidc',
    preset: null,
    description: 'Most sign-in services and identity providers. Try this first.',
    scopes: OIDC_SCOPES,
    groups: true,
    steps: [
      'At your provider, create a client (often called an application) with a client secret.',
      'Allow `{redirect}` as a redirect URI.',
      "Copy the client ID and secret, and find the issuer URL: the provider's settings are at the issuer plus `/.well-known/openid-configuration`.",
    ],
    docs: 'https://openid.net/developers/how-connect-works/',
  },
  {
    id: 'other-oauth2',
    label: 'Other OAuth2',
    group: 'Other',
    kind: 'oauth2',
    preset: null,
    description: 'Only for providers without OpenID Connect. It needs a few more details.',
    scopes: '',
    groups: true,
    steps: [
      'At your provider, register an OAuth2 client with `{redirect}` as its redirect URI.',
      "Find its authorization, token and user URLs in the provider's documentation.",
      'Under **Advanced → User fields**, say where its user answer has the ID, name and email. Try signing in shows the fields it sends.',
    ],
    docs: 'https://oauth.net/2/',
  },
]

export const GROUPS = ['Self-hosted', 'Online accounts', 'Other'] as const

/** The entry for a saved provider. Older GitHub and Discord ones have no preset stored. */
export function presetOf(kind: ProviderKind, preset: string | null | undefined): Preset {
  const byId = (id: string) => PRESETS.find((p) => p.id === id)
  if (preset) {
    return byId(preset) ?? byId('other-oidc')!
  }
  if (kind === 'github' || kind === 'discord') {
    return byId(kind)!
  }
  return byId(kind === 'oauth2' ? 'other-oauth2' : 'other-oidc')!
}
