# Local PocketBase

PocketBase is the local auth provider for Phase 1 of the auth rollout.

## Install

Download PocketBase for macOS from:

https://pocketbase.io/docs/

Put the `pocketbase` binary in this directory or make it available on your `PATH`.

## TLS

For local HTTPS, create a self-signed certificate in this directory:

```bash
mkcert localhost 127.0.0.1 ::1
```

Or use another self-signed certificate pair and set:

```bash
POCKETBASE_CERT_PATH=pocketbase/localhost+2.pem
POCKETBASE_KEY_PATH=pocketbase/localhost+2-key.pem
```

Your browser may require a local exception for self-signed certificates.

## Run

From the repository root:

```bash
make pocketbase-run
```

Default URL:

```text
https://127.0.0.1:8090
```

Create the first admin user in the PocketBase admin UI, then create/enable the `users` auth collection.

## Rust app configuration

Copy `music_browser/.env.example` to `music_browser/.env` and set:

```env
POCKETBASE_URL=https://127.0.0.1:8090
POCKETBASE_JWT_SECRET=<PocketBase token signing secret>
AUTH_COOKIE_SECURE=true
```

Keep real secrets out of git.
