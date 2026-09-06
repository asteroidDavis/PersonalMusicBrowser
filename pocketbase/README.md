# Local PocketBase

PocketBase is the local auth provider for Phase 1 of the auth rollout.

## Install

Download PocketBase for macOS from:

https://pocketbase.io/docs/

Put the `pocketbase` binary in this directory or make it available on your `PATH`.

## Local Development (HTTP)

For local development, PocketBase runs over HTTP. This is secure enough for localhost and avoids certificate issues.

```bash
make pocketbase-run-insecure
```

Default URL: `http://127.0.0.1:8090`

Make sure your `.env` matches:
```env
POCKETBASE_URL=http://127.0.0.1:8090
```

## HTTPS (Beta/Untested)

To run PocketBase with HTTPS locally, generate a self-signed certificate:

```bash
make pocketbase-cert
```

This creates `pocketbase/localhost.crt` and `pocketbase/localhost.key`.

Then run:

```bash
make pocketbase-run
```

**Note:** This HTTPS mode is beta and untested. Your browser may require a local exception for self-signed certificates.

For production deployment, refer to PocketBase documentation for recommended TLS setup.

Create the first admin user in the PocketBase admin UI, then create/enable the `users` auth collection.

## Users auth collection

PocketBase admins and app users are separate identities. The first superuser/admin account is for the PocketBase admin UI and cannot be used as a normal app user.

For local signup through the Rust app:

1. Open the PocketBase admin UI.
2. Confirm the `users` auth collection exists.
3. Enable email/password authentication for the `users` auth collection.
4. Set the `users` collection create rule to allow safe public registration for local testing.

For local-only development, a permissive create rule can be:

```text
@request.auth.id = ""
```

If PocketBase reports user ID conflicts, leave the record `id` field blank. PocketBase should generate user record IDs; the Rust app does not send an `id` during signup.

Before deploying, replace the permissive create rule with a hardened registration policy that includes email verification, rate limiting, and abuse controls.

## ReBAC collections

The Rust app stores music resources in SQLite and stores access metadata in PocketBase. Create these base collections before enabling `AUTH_REQUIRE_LOGIN=true` for real use.

### `shares`

Create a base collection named `shares` with these fields:

| Field | Type | Required | Notes |
|---|---|---:|---|
| `user_id` | text | yes | PocketBase `users` record id receiving access |
| `resource_type` | text | yes | Resource type such as `song`, `album`, or `instrument` |
| `resource_id` | text | yes | SQLite id for the resource |
| `access_level` | select | yes | Values: `viewer`, `editor`, `admin` |
| `created_by` | text | yes | PocketBase `users` record id that created the share |

Recommended indexes:

```text
user_id, resource_type, resource_id
resource_type, resource_id
created_by
```

Starter API rules:

```text
List/Search: @request.auth.id != "" && (user_id = @request.auth.id || created_by = @request.auth.id)
View:        @request.auth.id != "" && (user_id = @request.auth.id || created_by = @request.auth.id)
Create:      @request.auth.id != "" && created_by = @request.auth.id
Update:      @request.auth.id != "" && created_by = @request.auth.id
Delete:      @request.auth.id != "" && created_by = @request.auth.id
```

### `groups`

Create a base collection named `groups` with these fields:

| Field | Type | Required | Notes |
|---|---|---:|---|
| `name` | text | yes | Display name |
| `description` | text | no | Optional description |
| `owner_id` | text | yes | PocketBase `users` record id that owns the group |

Starter API rules:

```text
List/Search: @request.auth.id != ""
View:        @request.auth.id != ""
Create:      @request.auth.id != "" && owner_id = @request.auth.id
Update:      @request.auth.id != "" && owner_id = @request.auth.id
Delete:      @request.auth.id != "" && owner_id = @request.auth.id
```

### `group_memberships`

Create a base collection named `group_memberships` with these fields:

| Field | Type | Required | Notes |
|---|---|---:|---|
| `group_id` | text | yes | PocketBase `groups` record id |
| `user_id` | text | yes | PocketBase `users` record id |
| `role` | select | yes | Values: `owner`, `admin`, `member`, `viewer` |

Starter API rules:

```text
List/Search: @request.auth.id != "" && user_id = @request.auth.id
View:        @request.auth.id != "" && user_id = @request.auth.id
Create:      @request.auth.id != ""
Update:      @request.auth.id != ""
Delete:      @request.auth.id != ""
```

### `group_shares`

Create a base collection named `group_shares` with these fields:

| Field | Type | Required | Notes |
|---|---|---:|---|
| `group_id` | text | yes | PocketBase `groups` record id receiving access |
| `resource_type` | text | yes | Resource type such as `song`, `album`, or `instrument` |
| `resource_id` | text | yes | SQLite id for the resource |
| `access_level` | select | yes | Values: `viewer`, `editor`, `admin` |
| `created_by` | text | yes | PocketBase `users` record id that created the group share |

Starter API rules:

```text
List/Search: @request.auth.id != ""
View:        @request.auth.id != ""
Create:      @request.auth.id != "" && created_by = @request.auth.id
Update:      @request.auth.id != "" && created_by = @request.auth.id
Delete:      @request.auth.id != "" && created_by = @request.auth.id
```

The starter rules are intentionally simple for local rollout. Before production, tighten group membership and group share mutations so only group owners/admins can manage membership and only resource admins can share resources.

The `shares`, `groups`, `group_memberships`, and `group_shares` collections above are created automatically by `pb_migrations/1788724839_rebac_setup.js` the first time PocketBase starts against a data directory (no manual dashboard setup required); the field/rule tables here document what that migration creates.

## Migrations

`pb_migrations/` is applied automatically on `pocketbase serve` (and via `pocketbase migrate up`). It contains:

- `1788724839_rebac_setup.js` — creates the ReBAC collections described above.
- `1788730000_ci_test_seed.js` — seeds two known test users (`acl-test-user-1@example.com` / `acl-test-user-2@example.com`) used by `music_browser/tests/pocketbase_client_integration_tests.rs`. This is a no-op unless the `pocketbase` process has `PB_TEST_SEED=true` set, so it never runs against real dev/prod databases. See `music_browser/scripts/run-pocketbase-integration-tests.sh` for how CI and the pre-commit hook run PocketBase with this seed applied.

## Rust app configuration

Copy `music_browser/.env.example` to `music_browser/.env` and set:

```env
# Local development (HTTP)
POCKETBASE_URL=http://127.0.0.1:8090
POCKETBASE_JWT_SECRET=<PocketBase token signing secret>
AUTH_COOKIE_SECURE=true
```

For production with HTTPS:
```env
POCKETBASE_URL=https://your-domain.com
POCKETBASE_JWT_SECRET=<PocketBase token signing secret>
# Only if using a self-signed or private CA cert
POCKETBASE_CA_CERT=/path/to/ca-cert.pem
AUTH_COOKIE_SECURE=true
```

Keep real secrets out of git.
