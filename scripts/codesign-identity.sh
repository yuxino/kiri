#!/usr/bin/env bash
set -euo pipefail

# Prints one exact, private-key-backed code-signing certificate fingerprint.
# Developer ID is never selected automatically; release credentials must be
# an explicit choice.

fail() {
  echo "kiri signing: $*" >&2
  exit 1
}

requested_identity="${1:-}"
if [[ "$requested_identity" == "-" ]]; then
  fail "ad-hoc signing is not allowed for a macOS app that can request privacy permissions"
fi

identity_list="$(/usr/bin/security find-identity -v -p codesigning 2>/dev/null || true)"

matching_count() {
  printf '%s\n' "$1" | /usr/bin/awk 'NF { count++ } END { print count + 0 }'
}

if [[ -n "$requested_identity" ]]; then
  matches="$({
    printf '%s\n' "$identity_list" | /usr/bin/awk -v requested="$requested_identity" '
      /^[[:space:]]*[0-9]+\)/ {
        fingerprint = toupper($2)
        name = $0
        sub(/^[^"]*"/, "", name)
        sub(/"[[:space:]]*$/, "", name)
        if (fingerprint == toupper(requested) || name == requested) print fingerprint
      }
    '
  } || true)"
  case "$(matching_count "$matches")" in
    1) printf '%s\n' "$matches"; exit 0 ;;
    0) fail "the requested identity is not a valid code-signing identity: $requested_identity" ;;
    *) fail "multiple identities match '$requested_identity'; use the certificate fingerprint" ;;
  esac
fi

apple_development="$({
  printf '%s\n' "$identity_list" \
    | /usr/bin/awk '/"Apple Development:/ { print toupper($2) }'
} || true)"
case "$(matching_count "$apple_development")" in
  1) printf '%s\n' "$apple_development"; exit 0 ;;
  0) ;;
  *) fail "multiple Apple Development identities are valid; set KIRI_SIGNING_IDENTITY to one fingerprint" ;;
esac

shared_local="$({
  printf '%s\n' "$identity_list" \
    | /usr/bin/awk '/"mimi Local Development"[[:space:]]*$/ { print toupper($2) }'
} || true)"
case "$(matching_count "$shared_local")" in
  1) printf '%s\n' "$shared_local"; exit 0 ;;
  0) ;;
  *) fail "multiple 'mimi Local Development' identities are valid; set KIRI_SIGNING_IDENTITY to one fingerprint" ;;
esac

cat >&2 <<'EOF'
kiri signing: no stable code-signing identity is available.

Install an Apple Development identity, keep the shared long-lived
"mimi Local Development" identity, or set KIRI_SIGNING_IDENTITY to an
existing certificate fingerprint. Ad-hoc signing is intentionally rejected.
EOF
exit 1
