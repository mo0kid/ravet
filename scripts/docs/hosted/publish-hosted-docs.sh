#!/usr/bin/env bash
# Publish a generated hosted-documentation snapshot by directory swap.
#
# Usage:
#   publish-hosted-docs.sh publish <generated-docs-directory>
#   publish-hosted-docs.sh rollback
#
# The remote location is a dedicated release root, normally the home directory
# of a docs-only FTPS account. It holds three sibling directories:
#   current/   served at /ravet/docs/ through a one-time server symlink
#   previous/  the snapshot replaced by the last publish (one-step rollback)
#   next/      upload staging; never served
# Publishing uploads the complete snapshot to next/, checks it, then renames
# current/ -> previous/ and next/ -> current/. A failure before the renames
# leaves the live snapshot untouched. Rollback swaps current/ and previous/, so
# running it twice restores the original snapshot.
#
# Environment:
#   DOCS_REMOTE_URL   ftp://host[:port] for FTPS, or file:///path for a local
#                     dry run. Required.
#   DOCS_FTP_USER     FTP account name. Required for ftp:// URLs.
#   LFTP_PASSWORD     FTP account password, read by lftp --env-password.
#   DOCS_EXPECTED_COMMIT  If set, version.json must name this commit.
#   DOCS_PARALLEL     Concurrent uploads (default 4; shared hosts limit them).
set -euo pipefail

usage() { sed -n '2,8p' "$0" >&2; exit 2; }
[ $# -ge 1 ] || usage
action=$1
: "${DOCS_REMOTE_URL:?DOCS_REMOTE_URL is required}"
parallel=${DOCS_PARALLEL:-4}
[[ $parallel =~ ^[1-9][0-9]?$ ]] || { echo "DOCS_PARALLEL must be 1-99" >&2; exit 2; }

# lftp has its own command quoting; accept only values that need none.
plain() {
  [[ $1 =~ ^[A-Za-z0-9@._:/+-]+$ ]] || { echo "Unsupported characters in $2" >&2; exit 2; }
}
plain "$DOCS_REMOTE_URL" DOCS_REMOTE_URL

case $DOCS_REMOTE_URL in
  ftp://*)
    : "${DOCS_FTP_USER:?DOCS_FTP_USER is required for FTPS}"
    : "${LFTP_PASSWORD:?LFTP_PASSWORD is required for FTPS}"
    # Explicit TLS for control and data; verify the server certificate.
    settings='set ftp:ssl-force true; set ftp:ssl-protect-data true; set ftp:ssl-protect-list true; set ssl:verify-certificate true; set ftp:passive-mode true;'
    plain "$DOCS_FTP_USER" DOCS_FTP_USER
    open_cmd="open --env-password -u $DOCS_FTP_USER $DOCS_REMOTE_URL"
    ;;
  file:///*)
    settings=''
    open_cmd="open $DOCS_REMOTE_URL"
    ;;
  *) echo "DOCS_REMOTE_URL must start with ftp:// or file:///" >&2; exit 2 ;;
esac
settings="set cmd:fail-exit true; set net:max-retries 3; set net:timeout 30; set net:reconnect-interval-base 5; $settings"

remote() {
  # Runs lftp commands in the release root; non-zero exit on any failure.
  lftp -c "$settings $open_cmd; $*"
}

remote_exists() {
  # cls fails when the path is missing; listing output is discarded.
  remote "cls -d $1" > /dev/null 2>&1
}

check_snapshot() {
  local dir=$1
  [ -d "$dir" ] || { echo "Snapshot directory not found: $dir" >&2; return 1; }
  local required
  for required in index.html version.json manifest.json search.json; do
    [ -f "$dir/$required" ] || { echo "Snapshot is missing $required" >&2; return 1; }
  done
  # Pages load the shared navigation file named by the manifest.
  python3 - "$dir" <<'PY' || return 1
import json, pathlib, re, sys
root = pathlib.Path(sys.argv[1])
name = json.loads((root / 'manifest.json').read_text()).get('navigation')
if name is not None and not (re.fullmatch(r'nav-[0-9a-f]{12}\.json', name) and (root / name).is_file()):
    sys.exit(f'Snapshot is missing its navigation file {name}')
PY
  # Only static documentation may be published. In particular, refuse
  # server-executed or server-configuring files such as .php and .htaccess.
  local unexpected
  unexpected=$(cd "$dir" && find . \( -type l -o -name '.*' ! -name . \) -print -o -type f \
    ! -name '*.html' ! -name '*.json' ! -name '*.css' ! -name '*.js' ! -name '*.svg' \
    ! -name '*.png' ! -name '*.jpg' ! -name '*.webp' ! -name '*.txt' ! -name 'LICENSE' -print)
  if [ -n "$unexpected" ]; then
    echo "Refusing to publish unexpected files:" >&2
    printf '%s\n' "$unexpected" | head -20 >&2
    return 1
  fi
  local commit
  commit=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["commit"] or "")' "$dir/version.json")
  [[ $commit =~ ^[0-9a-f]{40}$ ]] || { echo "version.json has no commit" >&2; return 1; }
  if [ -n "${DOCS_EXPECTED_COMMIT:-}" ] && [ "$commit" != "$DOCS_EXPECTED_COMMIT" ]; then
    echo "version.json names $commit, expected $DOCS_EXPECTED_COMMIT" >&2
    return 1
  fi
  echo "Snapshot $commit: $(cd "$dir" && find . -type f | wc -l | tr -d ' ') files"
}

publish() {
  local dir
  dir=$(cd "$1" && pwd)
  plain "$dir" "the snapshot path"
  check_snapshot "$dir"
  # Discard staging left by an interrupted run; current/ is not touched.
  remote "rm -r -f next"
  remote "mkdir next"
  # version.json goes last: its presence marks a complete upload.
  remote "mirror --reverse --no-perms --parallel=$parallel --exclude-glob version.json $dir next"
  remote "put $dir/version.json -o next/version.json"
  remote "cls -d next/index.html next/version.json next/manifest.json next/search.json" > /dev/null
  # Free space only once the new snapshot is complete.
  remote "rm -r -f previous"
  if remote_exists current; then
    remote "mv current previous"
  fi
  if ! remote "mv next current"; then
    echo "Swap failed; restoring the previous snapshot" >&2
    remote_exists previous && remote "mv previous current"
    return 1
  fi
  echo "Published to current/; the replaced snapshot is in previous/"
}

rollback() {
  remote_exists previous || { echo "No previous snapshot to restore" >&2; return 1; }
  remote "rm -r -f rollback"
  remote "mv current rollback"
  if ! remote "mv previous current"; then
    remote "mv rollback current"
    echo "Rollback failed; current snapshot restored" >&2
    return 1
  fi
  remote "mv rollback previous"
  echo "Restored previous/ as current/; the replaced snapshot is now previous/"
}

case $action in
  publish) [ $# -eq 2 ] || usage; publish "$2" ;;
  rollback) [ $# -eq 1 ] || usage; rollback ;;
  *) usage ;;
esac
