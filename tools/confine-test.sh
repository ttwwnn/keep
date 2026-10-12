#!/usr/bin/env bash
#
# keep-confine: o programa de um workspace com pasta permitida só lê e grava
# nela (e no mínimo de que a IA precisa); sem pasta, roda como sempre; com a
# pasta sumida, não sobe. Sandbox de verdade (sandbox-exec), sem app. Segundos.
#
#   tools/confine-test.sh
#
# O helper é ~/.local/bin/keep-confine (KEEP_CONFINE_BIN o troca).
set -uo pipefail
BIN=${KEEP_CONFINE_BIN:-$HOME/.local/bin/keep-confine}
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
run() { KEEP_WORKSPACE="$1" "$BIN" /bin/sh -c "$2" 2>&1 | grep -v '^Keep: '; }

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

echo "typing claude, as the shell function runs it"
# Where the AI's own binary is: readable by it (it lives in ~/.local/bin for real).
mkdir -p "$WORK/permitida/bin"
cat > "$WORK/permitida/bin/claude" <<'FAKE'
#!/bin/sh
cat "$HOME/.zshrc" >/dev/null 2>&1 && echo leu-a-casa || echo nao-leu-a-casa
cat "$KEEP_CONFINED_TO/a.txt" 2>&1
FAKE
chmod +x "$WORK/permitida/bin/claude"
printf '{"presa": "%s"}\n' "$WORK/permitida" > "$KEEP_CONFINE_CONFIG"
FUNCAO=${KEEP_CLAUDE_FUNCAO:-$HOME/.config/zsh/claude-funcao.zsh}
as_function() {  # as_function <workspace>
    KEEP_WORKSPACE="$1" PATH="$WORK/permitida/bin:$PATH" HOME="$HOME" zsh -c "source '$FUNCAO'; claude" 2>&1 | grep -v '^Keep: '
}
check "the AI of a workspace with a folder is held to it" "nao-leu-a-casa
dentro" "$(as_function presa)"
check "the AI of another workspace is not" "leu-a-casa" "$(as_function livre | head -1)"

echo "a pasta sumiu"
printf '{"presa": "%s"}\n' "$WORK/nao-existe" > "$KEEP_CONFINE_CONFIG"
OUT=$(KEEP_WORKSPACE=presa "$BIN" /bin/echo subiu 2>&1)
check "o programa não sobe" "no" "$(echo "$OUT" | grep -qx subiu && echo yes || echo no)"
check "e diz por quê" "yes" "$(echo "$OUT" | grep -q 'não existe' && echo yes || echo no)"

echo
echo "passed $PASSED, failed $FAILED"
[ "$FAILED" -eq 0 ]
