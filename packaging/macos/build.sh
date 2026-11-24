#!/bin/sh
# Builds Meeting Assistant.app, signs it, and puts it into a disk image to install it from.
# docs/technical/packaging.md describes the steps and the signing identities.
set -eu

usage() {
    cat <<'EOF'
Usage: packaging/macos/build.sh [--identity <name> | --ad-hoc] [--notary-profile <profile>]

  --identity <name>          Code-signing identity in the keychain to sign with
                             (default: "Meeting Assistant Local Signing")
  --ad-hoc                   Sign ad hoc. macOS then drops the app's permissions at every update
  --notary-profile <profile> Notarize the disk image with the notarytool keychain profile of that name
                             and staple the ticket; needs a Developer ID Application identity

Writes target/macos/Meeting Assistant.app and target/macos/Meeting-Assistant-<version>.dmg.
EOF
}

identity="Meeting Assistant Local Signing"
notary_profile=""
while [ $# -gt 0 ]; do
    case "$1" in
        --identity) [ $# -ge 2 ] || { usage >&2; exit 2; }; identity=$2; shift 2 ;;
        --ad-hoc) identity="-"; shift ;;
        --notary-profile) [ $# -ge 2 ] || { usage >&2; exit 2; }; notary_profile=$2; shift 2 ;;
        -h | --help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
if [ -n "$notary_profile" ] && [ "$identity" = "-" ]; then
    echo "Notarization needs a Developer ID Application identity, not an ad-hoc signature." >&2
    exit 2
fi

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
out="$root/target/macos"
app="$out/Meeting Assistant.app"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)
[ -n "$version" ] || { echo "Cannot read the version from $root/Cargo.toml" >&2; exit 1; }

# Check the identity first, so that nothing is built for a signature that cannot be made.
if [ "$identity" != "-" ] && ! /usr/bin/security find-identity -p codesigning | grep -F "\"$identity\"" >/dev/null; then
    cat >&2 <<EOF
The keychain has no code-signing identity named "$identity".
Create it as docs/technical/packaging.md says, name another with --identity, or sign with --ad-hoc.
EOF
    exit 1
fi

echo "Building Meeting Assistant $version"
cargo build --release --locked -p meeting-assistant --manifest-path "$root/Cargo.toml"

echo "Assembling $app"
rm -rf "$out"
mkdir -p "$app/Contents/MacOS"
cp "$root/target/release/meeting-assistant" "$app/Contents/MacOS/meeting-assistant"
sed "s/@VERSION@/$version/g" "$here/Info.plist" >"$app/Contents/Info.plist"
/usr/bin/plutil -lint -s "$app/Contents/Info.plist"

# The hardened runtime is what notarization requires; the app gets it with every identity, so that a build behaves
# the same whoever signs it. Under it the microphone needs the entitlement.
if [ -n "$notary_profile" ]; then timestamp="--timestamp"; else timestamp="--timestamp=none"; fi
echo "Signing with ${identity}"
/usr/bin/codesign --force --sign "$identity" --identifier com.meeting-assistant.app --options runtime \
    --entitlements "$here/entitlements.plist" "$timestamp" "$app"
/usr/bin/codesign --verify --strict "$app"

dmg="$out/Meeting-Assistant-$version.dmg"
staging="$out/disk-image"
mkdir -p "$staging"
/usr/bin/ditto "$app" "$staging/Meeting Assistant.app"
/usr/bin/hdiutil create -quiet -volname "Meeting Assistant" -srcfolder "$staging" -format UDZO -ov "$dmg"
rm -rf "$staging"

if [ -n "$notary_profile" ]; then
    /usr/bin/codesign --force --sign "$identity" --timestamp "$dmg"
    echo "Notarizing $dmg"
    /usr/bin/xcrun notarytool submit "$dmg" --keychain-profile "$notary_profile" --wait
    /usr/bin/xcrun stapler staple "$dmg"
fi

echo "$dmg"
