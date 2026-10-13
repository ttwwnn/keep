#!/usr/bin/env bash
#
# `keep ia confinar`: o programa de um workspace com pasta permitida só lê e
# grava nela (e no mínimo de que a IA precisa); sem pasta, roda como sempre;
# com a pasta sumida, não sobe; o que mandaria trabalho para um servidor de
# fundo é recusado. Sandbox de verdade (sandbox-exec), sem app. Segundos.
#
#   tools/confine-test.sh
#
# O `keep` é o do target/release deste repositório (KEEP_BIN o troca).
set -uo pipefail
cd "$(dirname "$0")/.."
KEEP=${KEEP_BIN:-$PWD/target/release/keep}
[ -x "$KEEP" ] || { echo "sem $KEEP: cargo build --release -p keep"; exit 1; }
# Under the home, which is what is denied, and outside what the AI is let keep.
WORK=$(mktemp -d "$HOME/.keep-confine-test-XXXXXX")
trap 'rm -rf "$WORK"' EXIT
PASSED=0
FAILED=0
check() {  # check <what> <wanted> <got>
    if [ "$3" = "$2" ]; then echo "  ok    $1"; PASSED=$((PASSED + 1))
    else echo "  FAIL  $1"; echo "        wanted: $2"; echo "        got:    $3"; FAILED=$((FAILED + 1)); fi
}

mkdir -p "$WORK/permitida" "$WORK/fora"
echo dentro > "$WORK/permitida/a.txt"
echo fora > "$WORK/fora/b.txt"
export KEEP_CONFINE_CONFIG=$WORK/confinement.json
printf '{"presa": "%s"}\n' "$WORK/permitida" > "$KEEP_CONFINE_CONFIG"
run() { KEEP_WORKSPACE="$1" "$KEEP" ia confinar -- /bin/sh -c "$2" 2>&1 | grep -v '^Keep: '; }

echo "um workspace com pasta"
check "lê na pasta" "dentro" "$(run presa "cat '$WORK/permitida/a.txt'")"
check "grava na pasta" "novo" "$(run presa "echo novo > '$WORK/permitida/c.txt'; cat '$WORK/permitida/c.txt'")"
check "não lê o que está fora" "cat: $WORK/fora/b.txt: Operation not permitted" "$(run presa "cat '$WORK/fora/b.txt'")"
check "não grava fora" "yes" "$(run presa "echo x > '$WORK/fora/d.txt'" >/dev/null; [ -e "$WORK/fora/d.txt" ] && echo no || echo yes)"
check "não lê a casa" "yes" "$(run presa 'cat "$HOME/.zshrc" >/dev/null && echo leu' | grep -q leu && echo no || echo yes)"
check "não lista a casa" "yes" "$(run presa 'ls "$HOME"' | grep -q Documents && echo no || echo yes)"
check "a casa do Claude Code continua lida" "yes" "$(run presa 'head -c 1 "$HOME/.claude/settings.json" >/dev/null && echo yes || echo no')"
check "sobe na pasta quando a aba está fora dela" "$(cd "$WORK/permitida" && pwd -P)" "$(cd "$WORK/fora" && run presa pwd)"
check "sabe a pasta" "$WORK/permitida" "$(run presa 'echo $KEEP_CONFINED_TO')"

echo "um workspace sem pasta"
check "lê onde quiser" "fora" "$(run livre "cat '$WORK/fora/b.txt'")"

echo "what would send work out of the sandbox"
printf '{"presa": "%s"}\n' "$WORK/permitida" > "$KEEP_CONFINE_CONFIG"
check "claude agents is refused" "1" "$(KEEP_WORKSPACE=presa "$KEEP" ia confinar -- claude agents >/dev/null 2>&1; echo $?)"
check "codex queue is refused" "1" "$(KEEP_WORKSPACE=presa "$KEEP" ia confinar -- codex queue x >/dev/null 2>&1; echo $?)"
mkdir -p "$WORK/permitida/bin"
printf '#!/bin/sh\necho "$@"\n' > "$WORK/permitida/bin/codex"; chmod +x "$WORK/permitida/bin/codex"
check "codex goes up with its server in its own process" "--no-daemon resume x" \
    "$(KEEP_WORKSPACE=presa "$KEEP" ia confinar -- "$WORK/permitida/bin/codex" resume x 2>/dev/null)"

echo "a pasta sumiu"
printf '{"presa": "%s"}\n' "$WORK/nao-existe" > "$KEEP_CONFINE_CONFIG"
OUT=$(KEEP_WORKSPACE=presa "$KEEP" ia confinar -- /bin/echo subiu 2>&1)
check "o programa não sobe" "no" "$(echo "$OUT" | grep -qx subiu && echo yes || echo no)"
check "e diz por quê" "yes" "$(echo "$OUT" | grep -q 'não existe' && echo yes || echo no)"

echo
echo "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
