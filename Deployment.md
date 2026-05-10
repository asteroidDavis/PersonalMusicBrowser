

I'll help you set up HTTPS locally for manual QA. Here's the plan:

1. Generate a self-signed certificate
2. Update main.rs to support HTTPS
3. Update CSRF configuration for HTTPS
4. Update test configuration
5. Add instructions to .env.example

Let me start:































All tests pass. Here are the instructions for setting up HTTPS locally for manual QA:

**Steps to enable HTTPS locally:**

1. **Generate self-signed certificate:**
   ```bash
   cd music_browser
   ./generate-cert.sh
   ```
   This creates `certs/server.crt` and `certs/server.key` valid for 365 days.

2. **Trust the certificate in your browser:**
   - **macOS:** Open `certs/server.crt` in Keychain Access, add to System, and set "Always Trust"
   - **Chrome:** Import `certs/server.crt` in Settings > Privacy and security > Manage certificates
   - **Firefox:** Import in Preferences > Privacy & Security > Certificates > View Certificates > Import

3. **Configure environment variables:**
   Add to your `.env` file:
   ```
   HTTPS_ENABLED=true
   SSL_CERT_PATH=./certs/server.crt
   SSL_KEY_PATH=./certs/server.key
   CSRF_COOKIE_SECURE=true
   BIND_ADDR=127.0.0.1:3000
   ```

4. **Run the app:**
   ```bash
   make rust-run
   ```
   The app will now listen on `https://127.0.0.1:3000`

**Configuration details:**
- When `HTTPS_ENABLED=true`, the app uses `secure=true` and `same_site=Strict` for CSRF cookies
- When `HTTPS_ENABLED=false` (default), the app uses `secure=false` and `same_site=Lax` for HTTP
- Tests use `secure=true` and `same_site=Strict` to match the HTTPS configuration

**Note:** PocketBase does not need to use HTTPS for CSRF to work correctly - only the Rust app needs HTTPS for secure cookie handling.