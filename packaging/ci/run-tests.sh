#!/bin/sh
# Testele in CI (Linux / FreeBSD): la esec, erorile de compilare si testele cazute apar ca adnotari
# GitHub (vizibile in pagina run-ului si prin API fara autentificare), nu doar in jurnalul complet.
cargo test --release --no-fail-fast > test.log 2>&1
code=$?
tail -n 300 test.log
if [ "$code" -ne 0 ]; then
    enc() { awk '{ gsub(/%/, "%25"); printf "%s%%0A", $0 }'; }
    comp=$(grep -E "^error(\[E[0-9]+\])?:" -A12 test.log | head -n 80 | enc)
    [ -n "$comp" ] && echo "::error title=Compile errors::$comp"
    fails=$(grep -E "^---- .* stdout ----|panicked at|^test .* FAILED|assertion|left:|right:" -A2 test.log | head -n 120 | enc)
    [ -n "$fails" ] && echo "::error title=Failing tests::$fails"
    summary=$(grep -E "^test result:|^error: test failed|could not compile" test.log | head -n 30 | enc)
    [ -n "$summary" ] && echo "::error title=Summary::$summary"
fi
# a silent "0 tests ran" (bad filter / changed invocation) must not pass CI
if ! grep -q '^test result:' test.log; then
    echo "::error::no tests ran - check the cargo test invocation"
    exit 1
fi
exit $code
