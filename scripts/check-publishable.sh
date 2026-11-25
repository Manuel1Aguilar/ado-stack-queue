#!/usr/bin/env bash
# Fails (exit 1) if a tracked file holds something that must not be published:
# private keys, tokens, tailnet addresses, local volume paths or personal emails.
# Run from anywhere inside the repo: bash scripts/check-publishable.sh
# Pattern lines are built from fragments so this file does not match itself.
set -u
cd "$(git rev-parse --show-toplevel)"

dash='-----'
vol='/mnt/HC''_Volume_'
checks=(
  "private key header|${dash}BEGIN [A-Z ]*PRIVATE KEY"
  "GitHub token|(ghp|gho|ghs|ghu)_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}"
  "AWS access key|AKIA[0-9A-Z]{16}"
  "Slack token|xox[abprs]-[A-Za-z0-9-]{10,}"
  "credential assignment|(bearer|password|passwd|secret|api_key|apikey|token|pat)[\"']?[[:space:]]*[:=][[:space:]]*[\"']?[A-Za-z0-9+/_-]{32,}"
  "tailnet address (CGNAT range)|(^|[^0-9.])100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}"
  "local volume path|${vol}"
)
email_rx='[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}'
email_ok='(^|[^A-Za-z0-9._%+-])(noreply|no-reply)@|@([A-Za-z0-9-]+\.)*(example\.com|example\.invalid|users\.noreply\.github\.com)$'

mapfile -t files < <(git ls-files)
present=()
for f in "${files[@]}"; do [ -f "$f" ] && present+=("$f"); done

fail=0
report() { # label, grep -n output
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    echo "$line  [$1]"
    fail=1
  done <<<"$2"
}

for c in "${checks[@]}"; do
  label=${c%%|*}
  rx=${c#*|}
  report "$label" "$(grep -nIEH -i -e "$rx" -- "${present[@]}" | cut -c1-200)"
done

emails=$(grep -nIEHo -e "$email_rx" -- "${present[@]}" | grep -vE "$email_ok")
report "email address; use a noreply or example.com address" "$emails"

if [ "$fail" -ne 0 ]; then
  echo "check-publishable: found text that should not be published. Generalize it (placeholder path, example.com address) and rerun." >&2
  exit 1
fi
echo "check-publishable: clean (${#present[@]} files)"
