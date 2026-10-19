#!/usr/bin/env sh
# curl -fsSL https://raw.githubusercontent.com/6ixGODD/mine/master/scripts/bootstrap.sh | sh
# Standalone bootstrap: HTTPS curl + native tar, no Python/Rust dependency.
set -eu
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar >/dev/null 2>&1 || die "tar is required"
export LC_ALL=C
umask 077
ref="${MINE_REF:-latest}"
bin_dir="${MINE_BIN_DIR:-$HOME/.local/bin}"
account="${MINE_RELEASE_ACCOUNT:-6ixGODD}"
repo="${MINE_RELEASE_REPO:-mine}"
for identity in "$account" "$repo"; do
  case "$identity" in ''|.|..|*[!A-Za-z0-9_.-]*) die "invalid release account/repository" ;; esac
done
# BEGIN MINE PLATFORMS
case "$(uname -s):$(uname -m)" in
  Linux:x86_64) asset="mine-linux-x86_64.tar.gz"; bin_name="mine" ;;
  *) die "unsupported platform: $(uname -s) $(uname -m)" ;;
esac
# END MINE PLATFORMS
mkdir -p "$bin_dir"
bin_dir="$(cd "$bin_dir" && pwd -P)"
destination="$bin_dir/$bin_name"
[ ! -L "$destination" ] || die "installation target is a symlink"
[ ! -e "$destination" ] || [ -f "$destination" ] || die "installation target is not regular"
stage="$bin_dir/.mine-binary-swap"
mkdir "$stage" 2>/dev/null || die "pending/busy binary swap at $stage; inspect candidate/previous before retry"
printf '%s\n' "destination=$destination" 'candidate=candidate' 'backup=previous' >"$stage/record"
# Only delete this exact newly created directory before swap; an interrupted
# swap is deliberately retained with its record/old executable.
swapping=0
cleanup() { if [ "$swapping" = 0 ]; then rm -rf -- "$stage"; fi; }
trap cleanup 0
trap 'exit 1' HUP INT TERM

fetch() {
  request_url="$1"; output="$2"; byte_limit="$3"; mode="$4"
  case "$request_url" in https://*) ;; *) die "HTTPS required" ;; esac
  set -- --silent --show-error --location --max-redirs 5 --proto '=https' --proto-redir '=https' \
    --connect-timeout 10 --max-time 90 --retry 0 --max-filesize "$byte_limit" \
    --dump-header "$stage/headers" --output "$output" --write-out '%{http_code}\n%{url_effective}'
  [ "$mode" != head ] || set -- "$@" --head
  # ulimit also bounds chunked responses on older curl versions. Native tar
  # subprocesses below similarly limit CPU and listing/output file growth.
  (ulimit -f "$((byte_limit / 512 + 2))"; curl "$@" "$request_url") >"$stage/result" || die "network/timeout/oversized response; no automatic retry"
  status="$(sed -n '1p' "$stage/result")"
  case "$status" in
    200) ;;
    404) die "HTTP 404: no published release or requested asset; check tag/platform" ;;
    403|429)
      remaining="$(tr -d '\r' <"$stage/headers" | sed -n 's/^[Xx]-[Rr]atelimit-[Rr]emaining:[[:space:]]*\([0-9]*\)$/\1/p' | tail -n 1)"
      retry="$(tr -d '\r' <"$stage/headers" | sed -n 's/^[Rr]etry-[Aa]fter:[[:space:]]*\([0-9]*\)$/\1/p' | tail -n 1)"
      reset="$(tr -d '\r' <"$stage/headers" | sed -n 's/^[Xx]-[Rr]atelimit-[Rr]eset:[[:space:]]*\([0-9]*\)$/\1/p' | tail -n 1)"
      if [ "$status" = 429 ] || [ "$remaining" = 0 ] || [ -n "$retry" ]; then die "HTTP $status: rate limit; retry-after=$retry reset=$reset; no automatic retry"; fi
      die "HTTP $status: authorization/access denied (not proven rate limiting)" ;;
    401) die "HTTP 401: authorization/access denied" ;;
    *) die "HTTP $status: release request failed" ;;
  esac
  [ "$(wc -c <"$output" | tr -d ' ')" -le "$byte_limit" ] || die "response exceeded byte limit"
}

if [ "$ref" = latest ]; then
  # Official documented release redirect avoids JSON parsing and API quota.
  fetch "https://github.com/$account/$repo/releases/latest" "$stage/latest" 1048576 head
  resolved="$(sed -n '2p' "$stage/result")"
  prefix="https://github.com/$account/$repo/releases/tag/"
  case "$resolved" in "$prefix"*) tag="${resolved#"$prefix"}" ;; *) die "malformed latest release resolution" ;; esac
else tag="$ref"; fi
printf '%s\n' "$tag" | grep -Eq '^v?[0-9]{1,18}\.[0-9]{1,18}\.[0-9]{1,18}$' || die "malformed release tag"
version="${tag#v}"
base="https://github.com/$account/$repo/releases/download/$tag/$asset"
fetch "$base" "$stage/$asset" 134217728 get
fetch "$base.sha256" "$stage/checksum" 4096 get
sum="$(awk -v expected="$asset" 'NF == 2 && ($2 == expected || $2 == "*" expected) && length($1) == 64 && $1 !~ /[^0-9a-fA-F]/ {h=tolower($1);good++} {lines++} END {if(lines == 1 && good == 1) print h; else exit 1}' "$stage/checksum")" || die "malformed/ambiguous checksum or wrong filename"
if command -v sha256sum >/dev/null 2>&1; then actual="$(sha256sum "$stage/$asset" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then actual="$(shasum -a 256 "$stage/$asset" | awk '{print $1}')"
else die "SHA-256 tool required"; fi
[ "$sum" = "$actual" ] || die "archive SHA-256 mismatch"

(ulimit -t 30; ulimit -f 4096; tar -tzf "$stage/$asset" >"$stage/names") || die "malformed/oversized TAR archive"
(ulimit -t 30; ulimit -f 4096; tar -tvzf "$stage/$asset" >"$stage/types") || die "malformed TAR entry types"
# Refuse every link/device/FIFO before streaming the selected file. Conservative
# native listing grammar rejects control/escaped names, not a custom TAR parser.
while IFS= read -r entry; do
  case "$entry" in [-d]*) ;; *) die "archive link/special entry forbidden" ;; esac
done <"$stage/types"
count=0; selected=''
while IFS= read -r entry; do
  case "$entry" in ''|/*|*[!A-Za-z0-9_./-]*) die "unsafe archive path" ;; esac
  case "/$entry/" in */../*) die "archive path traversal" ;; esac
  if [ "${entry##*/}" = "$bin_name" ]; then count=$((count + 1)); selected="$entry"; fi
done <"$stage/names"
[ "$count" = 1 ] || die "missing/ambiguous native binary"
(ulimit -t 30; ulimit -f 262144; tar -xOzf "$stage/$asset" -- "$selected" >"$stage/candidate") || die "binary extraction failed"
[ -s "$stage/candidate" ] || die "empty native binary"
chmod 755 "$stage/candidate"
# Bounded version process, no shell fragments derived from archive metadata.
(ulimit -t 10; ulimit -f 8; exec "$stage/candidate" --version) >"$stage/version" 2>&1 &
version_pid=$!
(sleep 10; kill "$version_pid" 2>/dev/null || :) & watchdog=$!
version_ok=0
if wait "$version_pid"; then version_ok=1; fi
kill "$watchdog" 2>/dev/null || :
wait "$watchdog" 2>/dev/null || :
[ "$version_ok" = 1 ] && [ "$(cat "$stage/version")" = "mine $version" ] || die "candidate native version does not match pinned release"
candidate_hash="$(if command -v sha256sum >/dev/null 2>&1; then sha256sum "$stage/candidate"; else shasum -a 256 "$stage/candidate"; fi | awk '{print $1}')"
swapping=1
if [ -e "$destination" ]; then mv -- "$destination" "$stage/previous" || die "cannot preserve old executable; swap pending"; fi
if ! mv -- "$stage/candidate" "$destination"; then
  if [ -e "$stage/previous" ]; then mv -- "$stage/previous" "$destination" || die "replacement/rollback failed; recover $stage/previous"; fi
  die "replacement failed; prior install restored; swap pending at $stage"
fi
installed="$(if command -v sha256sum >/dev/null 2>&1; then sha256sum "$destination"; else shasum -a 256 "$destination"; fi | awk '{print $1}')"
[ "$installed" = "$candidate_hash" ] || {
  mv -- "$destination" "$stage/candidate" || die "verification failed; recover recorded previous executable"
  if [ -e "$stage/previous" ]; then mv -- "$stage/previous" "$destination" || die "rollback failed; recover $stage/previous"; fi
  die "installed binary verification failed; prior install restored; swap retained"
}
rm -f -- "$stage/previous"
rm -rf -- "$stage"
trap - 0 HUP INT TERM
printf 'mine %s binary installed; running Agent setup separately\n' "$tag"
export PATH="$bin_dir:$PATH"
export MINE_REF="$tag"
exec "$destination" setup "$@"
