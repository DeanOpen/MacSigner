#!/usr/bin/env bash
# CI only: import the Developer ID certificate into a temporary keychain and store
# notarization credentials. Exports, through $GITHUB_ENV:
#   MACSIGNER_SIGN_IDENTITY  - "Developer ID Application" identity hash (unset: ad-hoc)
#   MACSIGNER_KEYCHAIN       - keychain holding the identity and the notary profile
#   MACSIGNER_NOTARY_PROFILE - notarytool keychain profile (unset: no notarization)
# plus the APPLE_* variables `tauri build` reads to sign and notarize the app itself.
# Without MACOS_CERTIFICATE_P12 it does nothing, so forks still build (unsigned).
set -euo pipefail

if [ -z "${MACOS_CERTIFICATE_P12:-}" ]; then
  echo "::warning::MACOS_CERTIFICATE_P12 is not set; macOS builds will be ad-hoc signed and not notarized."
  exit 0
fi

keychain="$RUNNER_TEMP/signing.keychain-db"
keychain_password="$(uuidgen)"
printf '%s' "$MACOS_CERTIFICATE_P12" | base64 --decode > "$RUNNER_TEMP/cert.p12"
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$RUNNER_TEMP/cert.p12" -k "$keychain" -P "${MACOS_CERTIFICATE_PASSWORD:-}" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" $(security list-keychains -d user | tr -d '"')
rm -f "$RUNNER_TEMP/cert.p12"

identity="$(security find-identity -v -p codesigning "$keychain" | awk '/Developer ID Application/{print $2; exit}')"
if [ -z "$identity" ]; then
  echo "::error::No 'Developer ID Application' identity found in MACOS_CERTIFICATE_P12."
  exit 1
fi
{
  echo "MACSIGNER_SIGN_IDENTITY=$identity"
  echo "MACSIGNER_KEYCHAIN=$keychain"
  echo "APPLE_SIGNING_IDENTITY=$identity"
} >> "$GITHUB_ENV"

# Prefer an App Store Connect API key; fall back to Apple ID + app-specific password.
if [ -n "${APPSTORE_API_KEY_P8:-}" ] && [ -n "${APPSTORE_API_KEY_ID:-}" ] && [ -n "${APPSTORE_API_ISSUER_ID:-}" ]; then
  key_path="$RUNNER_TEMP/AuthKey_${APPSTORE_API_KEY_ID}.p8"
  printf '%s\n' "$APPSTORE_API_KEY_P8" > "$key_path"
  xcrun notarytool store-credentials macsigner-notary --keychain "$keychain" \
    --key "$key_path" --key-id "$APPSTORE_API_KEY_ID" --issuer "$APPSTORE_API_ISSUER_ID"
  {
    echo "APPLE_API_KEY=$APPSTORE_API_KEY_ID"
    echo "APPLE_API_ISSUER=$APPSTORE_API_ISSUER_ID"
    echo "APPLE_API_KEY_PATH=$key_path"
  } >> "$GITHUB_ENV"
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
  xcrun notarytool store-credentials macsigner-notary --keychain "$keychain" \
    --apple-id "$APPLE_ID" --password "$APPLE_APP_PASSWORD" --team-id "$APPLE_TEAM_ID"
  {
    echo "APPLE_ID=$APPLE_ID"
    echo "APPLE_PASSWORD=$APPLE_APP_PASSWORD"
    echo "APPLE_TEAM_ID=$APPLE_TEAM_ID"
  } >> "$GITHUB_ENV"
else
  echo "::warning::Notarization secrets missing; macOS builds will be signed but not notarized."
  exit 0
fi
echo "MACSIGNER_NOTARY_PROFILE=macsigner-notary" >> "$GITHUB_ENV"
