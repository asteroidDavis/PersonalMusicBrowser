#!/bin/bash
# Generate self-signed SSL certificate for local HTTPS development

CERT_DIR="./certs"
CERT_FILE="$CERT_DIR/server.crt"
KEY_FILE="$CERT_DIR/server.key"

# Create certs directory if it doesn't exist
mkdir -p "$CERT_DIR"

# Generate self-signed certificate valid for 365 days
openssl req -x509 -newkey rsa:4096 -nodes -keyout "$KEY_FILE" -out "$CERT_FILE" -days 365 -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,DNS:127.0.0.1"

echo "Certificate generated successfully:"
echo "  Certificate: $CERT_FILE"
echo "  Private Key: $KEY_FILE"
echo ""
echo "To trust this certificate in your browser:"
echo "  macOS: Open the .crt file in Keychain Access, add to System, and set 'Always Trust'"
echo "  Chrome: Import the .crt file in Settings > Privacy and security > Manage certificates"
echo ""
echo "Set the following environment variables:"
echo "  HTTPS_ENABLED=true"
echo "  SSL_CERT_PATH=$CERT_FILE"
echo "  SSL_KEY_PATH=$KEY_FILE"
echo "  CSRF_COOKIE_SECURE=true"
