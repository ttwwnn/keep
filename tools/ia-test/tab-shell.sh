#!/bin/bash
# The `$SHELL` of a scratch daemon in tools/account-menu-test.sh: what each
# new tab runs, taken in turn off the first line of $IA_TEST_QUEUE — a plain
# shell, the stand-in for Claude Code, or a program the AI menu must keep its
# hands off. A tab asked for with nothing queued is a shell.
#
# No rc files: the person's own shell setup is not what is under test, and it
# titles tabs and draws prompts of its own.
next=""
if [ -n "${IA_TEST_QUEUE:-}" ] && [ -s "$IA_TEST_QUEUE" ]; then
    next=$(head -n 1 "$IA_TEST_QUEUE")
    tail -n +2 "$IA_TEST_QUEUE" >"$IA_TEST_QUEUE.rest" && mv "$IA_TEST_QUEUE.rest" "$IA_TEST_QUEUE"
fi
case "$next" in
    claude) exec "$IA_TEST_BIN/claude" ;;
    sleep) exec /bin/sleep 100000 ;;
    *) exec /bin/zsh -f -i ;;
esac
