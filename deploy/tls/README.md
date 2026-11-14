# LAN TLS trust

The music browser is LAN-only, so no public CA will issue for
`music.home.arpa`. Instead a small private root CA signs the gateway's
certificate, and each device trusts that root once.

- **Root CA**: ECDSA P-256, 10 years, created on your laptop by
  `lan-ca.sh init`. Name-constrained to `.home.arpa` and your LAN subnet,
  so it can't vouch for any other site even if the key leaked.
- **Root key** (`~/.music-browser-ca/ca.key`): never copied to the server.
  Back it up offline (encrypted USB or password manager). You only need it
  to re-issue the leaf.
- **Leaf**: `music.home.arpa` + the host IP (+ optional `pb.music.home.arpa`),
  397 days, the longest browsers accept. Put a calendar reminder 2 weeks
  before it expires.

## Create and deploy

```sh
deploy/tls/lan-ca.sh init 10.0.0.0/24
deploy/tls/lan-ca.sh issue music.home.arpa 10.0.0.124 pb.music.home.arpa
deploy/tls/lan-ca.sh publish music.home.arpa      # kubectl pointed at the mini cluster
helm upgrade --install ... --set trust.configMap=lan-ca
deploy/tls/lan-ca.sh fingerprint                  # write this down / share it
```

`publish` creates `istio-ingress/music-browser-tls` (leaf + chain, private key)
and `music-browser/lan-ca` (public CA files only). Renewal is `issue` +
`publish` again; devices don't need to re-trust anything.

## Trust it on each device

Open `http://music.home.arpa/trust` (plain HTTP on purpose: the device
doesn't trust the HTTPS certificate yet). **Compare the SHA-256 fingerprint
shown there with the one from `lan-ca.sh fingerprint`** before installing.
Anyone on the LAN could serve a fake `/trust` page.

| Device | Steps |
| --- | --- |
| iPhone / iPad | Download the profile, then Settings → General → VPN & Device Management → install it. Then **Settings → General → About → Certificate Trust Settings → enable full trust** for "Music Browser LAN CA". Without this last step Safari still warns. |
| macOS | Open the profile (or `ca.crt`) and install it in System Settings → Privacy & Security → Profiles. Or: `sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ca.crt`. Safari and Chrome use it; Firefox: see below. |
| Windows | Double-click `ca.cer` → Install Certificate → Local Machine → "Trusted Root Certification Authorities". Or: `certutil -addstore -f Root ca.cer` in an admin shell. Edge/Chrome use it. |
| Linux (Debian/Ubuntu) | `sudo cp ca.crt /usr/local/share/ca-certificates/music-browser-lan-ca.crt && sudo update-ca-certificates`. Fedora: `/etc/pki/ca-trust/source/anchors/` + `sudo update-ca-trust`. Chrome on Linux uses its own NSS store: `certutil -d sql:$HOME/.pki/nssdb -A -t C,, -n music-browser-lan-ca -i ca.crt`. |
| Android | Settings → Security → Encryption & credentials → Install a certificate → **CA certificate** → pick `ca.cer`. Chrome trusts user CAs; many apps don't (fine, only the browser is needed). |
| Firefox (any OS) | Settings → Privacy & Security → Certificates → View Certificates → Authorities → Import `ca.crt`, tick "trust to identify websites". Or set `security.enterprise_roots.enabled=true` to use the OS store. |

Check: `curl --cacert ca.crt https://music.home.arpa/healthz` prints `ok`;
without `--cacert` it fails with error 60.

## Removing trust

Delete "Music Browser LAN CA" from the same place you added it. If the root
key is ever lost or exposed, run `init` with a new `LAN_CA_DIR`, re-issue,
re-publish and re-trust every device.
