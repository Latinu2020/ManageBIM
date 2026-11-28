#!/usr/bin/env bash
# Verificari de calitate "cu clichet" (CI, jobul quality):
#  - clippy: numarul de avertismente nu are voie sa creasca fata de packaging/ci/clippy-baseline.txt
#    (cand scade, CI spune noua valoare - se coboara referinta in acelasi commit)
#  - rustfmt: fisierele .rs ADAUGATE in push / PR trebuie sa fie formatate (codul vechi se formateaza treptat)
# Argument: commit-ul de baza (github.event.before / baza PR-ului); gol sau 000... = fara verificarea rustfmt.
set -u
base="${1:-}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
fail=0

# ---- clippy
cargo clippy --all-targets --message-format=short --color never > clippy.log 2>&1
code=$?
n=$(grep -cE '^[^ ]+\.rs:[0-9]+:[0-9]+: warning' clippy.log || true)
errs=$(grep -cE '^[^ ]+\.rs:[0-9]+:[0-9]+: error' clippy.log || true)
baseline=$(tr -dc '0-9' < packaging/ci/clippy-baseline.txt)
echo "clippy: exit $code, $n warnings (baseline $baseline), $errs errors"
# clippy n-a rulat pana la capat (dependinte, build script): ultimele randuri ca adnotari, nu "0 avertismente"
if [ "$code" -ne 0 ] || [ "$n" -eq 0 ]; then
  echo "::error::clippy did not finish (exit $code) - last lines of its output follow"
  tail -n 15 clippy.log | sed 's/^/::warning::clippy: /'
  fail=1
fi
if [ "$errs" -gt 0 ]; then
  grep -E '^[^ ]+\.rs:[0-9]+:[0-9]+: error' clippy.log | head -20 | sed 's/^/::error::/'
  fail=1
fi
if [ "$n" -gt "$baseline" ]; then
  echo "::error::clippy: $n warnings, more than the baseline $baseline - fix the new ones (cargo clippy --all-targets)"
  fail=1
elif [ "$n" -lt "$baseline" ]; then
  echo "::notice::clippy: $n warnings, fewer than the baseline $baseline - lower packaging/ci/clippy-baseline.txt to $n"
fi

# ---- rustfmt pe fisierele noi
if [ -n "$base" ] && ! echo "$base" | grep -qE '^0+$' && git cat-file -e "$base^{commit}" 2>/dev/null; then
  added=$(git diff --diff-filter=A --name-only "$base" HEAD -- '*.rs' | grep -v '^third_party/' || true)
  for f in $added; do
    if ! rustfmt --edition 2021 --check "$f" > /dev/null 2>&1; then
      echo "::error file=$f::New file is not formatted - run: rustfmt --edition 2021 $f"
      fail=1
    fi
  done
  echo "rustfmt: checked $(echo "$added" | grep -c . || true) new file(s)"
else
  echo "rustfmt: no base commit, new-file check skipped"
fi

exit $fail
