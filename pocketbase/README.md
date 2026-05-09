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
