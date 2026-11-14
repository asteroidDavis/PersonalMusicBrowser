#!/usr/bin/env bash
# LAN certificate authority for the music browser. Run on a laptop, not on
# the server: the root key never leaves LAN_CA_DIR (keep it offline/backed up).
#
#   deploy/tls/lan-ca.sh init 10.0.0.0/24            # root CA (10 years)
#   deploy/tls/lan-ca.sh issue music.home.arpa 10.0.0.124 [pb.music.home.arpa]
#   deploy/tls/lan-ca.sh mobileconfig                # iOS/macOS profile
#   deploy/tls/lan-ca.sh fingerprint
#   deploy/tls/lan-ca.sh publish music.home.arpa [namespace]   # needs kubectl
#
# The root is name-constrained to .home.arpa and the given subnet, so even a
# leaked root key can't mint certificates browsers accept for other sites.
set -euo pipefail

LAN_CA_DIR="${LAN_CA_DIR:-$HOME/.music-browser-ca}"
CA_NAME="${CA_NAME:-Music Browser LAN CA}"
LEAF_DAYS="${LEAF_DAYS:-397}"   # browsers reject longer leaf lifetimes
DOMAIN="${DOMAIN:-home.arpa}"

die() { echo "lan-ca: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null || die "$1 is required"; }

# 10.0.0.0/24 -> 10.0.0.0/255.255.255.0 (OpenSSL nameConstraints form)
cidr_to_mask() {
    local net="${1%/*}" bits="${1#*/}" mask="" i
    [[ "$1" == */* && "$bits" =~ ^[0-9]+$ && "$bits" -le 32 ]] || die "subnet must be CIDR, e.g. 10.0.0.0/24"
    for i in 1 2 3 4; do
        local n=$((bits >= 8 ? 8 : (bits > 0 ? bits : 0)))
        mask+="$((256 - (1 << (8 - n))))"
        [ "$i" -lt 4 ] && mask+="."
        bits=$((bits - n))
    done
    echo "$net/$mask"
}

cmd_init() {
    local subnet="${1:?usage: init <lan-subnet-cidr>}"
    [ ! -e "$LAN_CA_DIR/ca.key" ] || die "$LAN_CA_DIR/ca.key exists; refusing to overwrite"
    mkdir -p "$LAN_CA_DIR" && chmod 700 "$LAN_CA_DIR"
    cat >"$LAN_CA_DIR/ca.cnf" <<CNF
[req]
distinguished_name = dn
prompt = no
x509_extensions = v3_ca
[dn]
CN = $CA_NAME
O = $CA_NAME
[v3_ca]
basicConstraints = critical, CA:TRUE, pathlen:0
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
nameConstraints = critical, permitted;DNS:.$DOMAIN, permitted;DNS:$DOMAIN, permitted;IP:$(cidr_to_mask "$subnet")
CNF
    (umask 077 && openssl ecparam -name prime256v1 -genkey -noout -out "$LAN_CA_DIR/ca.key")
    openssl req -new -x509 -sha256 -days 3650 -key "$LAN_CA_DIR/ca.key" \
        -config "$LAN_CA_DIR/ca.cnf" -out "$LAN_CA_DIR/ca.crt"
    openssl x509 -in "$LAN_CA_DIR/ca.crt" -outform der -out "$LAN_CA_DIR/ca.cer"
    echo "root CA: $LAN_CA_DIR/ca.crt (key: $LAN_CA_DIR/ca.key - keep offline)"
    cmd_fingerprint
}

cmd_issue() {
    local host="${1:?usage: issue <hostname> <ip> [extra-dns...]}" ip="${2:?usage: issue <hostname> <ip>}"
    shift 2
    [ -e "$LAN_CA_DIR/ca.key" ] || die "run init first"
    local out="$LAN_CA_DIR/$host" san="DNS:$host,IP:$ip" d
    for d in "$@"; do san+=",DNS:$d"; done
    mkdir -p "$out"
    (umask 077 && openssl ecparam -name prime256v1 -genkey -noout -out "$out/tls.key")
    openssl req -new -sha256 -key "$out/tls.key" -subj "/CN=$host" -out "$out/tls.csr"
    cat >"$out/ext.cnf" <<EXT
basicConstraints = critical, CA:FALSE
keyUsage = critical, digitalSignature
extendedKeyUsage = serverAuth
subjectAltName = $san
authorityKeyIdentifier = keyid
subjectKeyIdentifier = hash
EXT
    openssl x509 -req -sha256 -days "$LEAF_DAYS" -in "$out/tls.csr" \
        -CA "$LAN_CA_DIR/ca.crt" -CAkey "$LAN_CA_DIR/ca.key" -CAcreateserial \
        -extfile "$out/ext.cnf" -out "$out/tls.crt"
    openssl verify -CAfile "$LAN_CA_DIR/ca.crt" "$out/tls.crt" >/dev/null || {
        rm -rf "$out"
        die "$san is outside the CA name constraints"
    }
    rm -f "$out/tls.csr"
    echo "leaf: $out/tls.crt + tls.key ($san, $LEAF_DAYS days)"
}

cmd_mobileconfig() {
    [ -e "$LAN_CA_DIR/ca.cer" ] || die "run init first"
    local b64 uuid1 uuid2
    b64="$(base64 <"$LAN_CA_DIR/ca.cer" | tr -d '\n')"
    uuid1="$(uuidgen 2>/dev/null || cat /proc/sys/kernel/random/uuid)"
    uuid2="$(uuidgen 2>/dev/null || cat /proc/sys/kernel/random/uuid)"
    cat >"$LAN_CA_DIR/music-browser-ca.mobileconfig" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>PayloadContent</key><array><dict>
    <key>PayloadType</key><string>com.apple.security.root</string>
    <key>PayloadVersion</key><integer>1</integer>
    <key>PayloadIdentifier</key><string>arpa.home.music-browser.ca</string>
    <key>PayloadUUID</key><string>$uuid1</string>
    <key>PayloadDisplayName</key><string>$CA_NAME</string>
    <key>PayloadContent</key><data>$b64</data>
  </dict></array>
  <key>PayloadType</key><string>Configuration</string>
  <key>PayloadVersion</key><integer>1</integer>
  <key>PayloadIdentifier</key><string>arpa.home.music-browser</string>
  <key>PayloadUUID</key><string>$uuid2</string>
  <key>PayloadDisplayName</key><string>$CA_NAME</string>
</dict></plist>
PLIST
    echo "profile: $LAN_CA_DIR/music-browser-ca.mobileconfig"
}

cmd_fingerprint() {
    openssl x509 -in "$LAN_CA_DIR/ca.crt" -noout -fingerprint -sha256 | sed 's/^.*=/SHA-256 /'
}

# Load the leaf into the gateway namespace and the *public* CA files into the
# app namespace. The root key is never sent anywhere.
cmd_publish() {
    local host="${1:?usage: publish <hostname> [namespace]}" ns="${2:-music-browser}"
    local gw_ns="${GATEWAY_NAMESPACE:-istio-ingress}" leaf="$LAN_CA_DIR/$host"
    need kubectl
    [ -e "$leaf/tls.crt" ] || die "run issue $host first"
    [ -e "$LAN_CA_DIR/music-browser-ca.mobileconfig" ] || cmd_mobileconfig >/dev/null
    cat "$leaf/tls.crt" "$LAN_CA_DIR/ca.crt" >"$leaf/fullchain.crt"
    kubectl create namespace "$gw_ns" --dry-run=client -o yaml | kubectl apply -f -
    kubectl -n "$gw_ns" create secret tls music-browser-tls \
        --cert="$leaf/fullchain.crt" --key="$leaf/tls.key" --dry-run=client -o yaml | kubectl apply -f -
    cmd_fingerprint >"$LAN_CA_DIR/fingerprint"
    kubectl create namespace "$ns" --dry-run=client -o yaml | kubectl apply -f -
    kubectl -n "$ns" create configmap lan-ca \
        --from-file="$LAN_CA_DIR/ca.crt" --from-file="$LAN_CA_DIR/ca.cer" \
        --from-file="$LAN_CA_DIR/music-browser-ca.mobileconfig" \
        --from-file="$LAN_CA_DIR/fingerprint" --dry-run=client -o yaml | kubectl apply -f -
    echo "published: secret $gw_ns/music-browser-tls, configmap $ns/lan-ca (set trust.configMap=lan-ca)"
}

need openssl
case "${1:-}" in
init | issue | mobileconfig | fingerprint | publish)
    c="$1"
    shift
    "cmd_$c" "$@"
    ;;
*) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
