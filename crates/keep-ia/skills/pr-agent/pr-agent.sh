#!/bin/sh
# O PR-Agent (github.com/ttwwnn/pr-agent-claude) sobre um diff, pela assinatura do Claude Code (`claude -p`):
# sem chave de API, sem token do GitHub e sem publicar nada. Instalado e mantido pelo Keep
# (`keep ia automacao pr-agent ligar|desligar`), que regrava esta cópia quando muda.
#
#   sh pr-agent.sh revisar|descrever|melhorar [--pr N|URL] [--base REF] [--modelo M] [--esforco E]
#   sh pr-agent.sh perguntar "pergunta" [as mesmas opções]
#
# Sem --pr: o ramo atual contra a base (padrão: o ramo principal do origin), com o que não foi commitado (arquivo
# novo só depois do `git add`). Com --pr: o diff do PR, pelo gh; a árvore entra como contexto só quando está na
# cabeça do PR e limpa. Imprime o resultado em Markdown e onde ficaram o .md, o .json (revisar) e o log.
# Saída: 0 = ok (também "nada a revisar"); 1 = falhou; 2 = uso errado.
# Variáveis: PR_AGENT_BIN (o executável), PR_AGENT_MODELO (claude-opus-5-5), PR_AGENT_ESFORCO (medium) e as do
# próprio PR-Agent, que valem por cima dos padrões daqui (CONFIG__MAX_MODEL_TOKENS, CONFIG__RESPONSE_LANGUAGE…).

set -u

uso() {
  echo "uso: sh $0 revisar|descrever|melhorar [--pr N|URL] [--base REF] [--modelo M] [--esforco E]" >&2
  echo "     sh $0 perguntar \"pergunta\" [as mesmas opções]" >&2
  exit 2
}

pergunta=""
case "${1:-}" in
  revisar) comando=review ;;
  descrever) comando=describe ;;
  melhorar) comando=improve ;;
  perguntar)
    comando=ask
    { [ $# -ge 2 ] && [ -n "$2" ]; } || uso
    pergunta=$2
    shift ;;
  *) uso ;;
esac
shift
pr=""
base=""
modelo=${PR_AGENT_MODELO:-claude-opus-5-5}
esforco=${PR_AGENT_ESFORCO:-medium}
while [ $# -gt 0 ]; do
  [ $# -ge 2 ] || uso
  case "$1" in
    --pr) pr=$2 ;;
    --base) base=$2 ;;
    --modelo) modelo=$2 ;;
    --esforco) esforco=$2 ;;
    *) uso ;;
  esac
  shift 2
done

bin=${PR_AGENT_BIN:-}
[ -n "$bin" ] || bin=$(command -v pr-agent 2>/dev/null) || bin=""
if [ -z "$bin" ] && [ -x "$HOME/.local/bin/pr-agent" ]; then bin=$HOME/.local/bin/pr-agent; fi
if [ -z "$bin" ]; then
  echo "O PR-Agent não está instalado. Instale com:" >&2
  echo "  uv tool install --python 3.12 'pr-agent @ git+https://github.com/ttwwnn/pr-agent-claude@main'" >&2
  exit 1
fi

tmp=${TMPDIR:-/tmp}
mkdir -p "${tmp%/}/pr-agent" || exit 1
dir=$(mktemp -d "${tmp%/}/pr-agent/$(date +%Y%m%d-%H%M%S).XXXXXX") || exit 1
diff=$dir/diff
# Sem a árvore certa, o PR-Agent roda numa pasta vazia e fica só com o patch.
onde=$dir

if [ -n "$pr" ]; then
  command -v gh >/dev/null 2>&1 || { echo "Sem o gh (GitHub CLI) não há como ler o PR." >&2; exit 1; }
  gh pr diff --color never "$pr" > "$diff" || { echo "O gh pr diff $pr falhou." >&2; exit 1; }
  cabeca=$(gh pr view "$pr" --json headRefOid --jq .headRefOid 2>/dev/null) || cabeca=""
  arvore=$(git rev-parse --show-toplevel 2>/dev/null) || arvore=""
  if [ -n "$arvore" ] && [ -n "$cabeca" ] && [ "$(git rev-parse HEAD 2>/dev/null)" = "$cabeca" ] \
    && git diff --quiet HEAD -- 2>/dev/null; then
    onde=$arvore
  fi
  alvo="o PR $pr"
else
  arvore=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "Fora de um repositório git: use --pr." >&2; exit 1; }
  cd "$arvore" || exit 1
  if [ -z "$base" ]; then
    padrao=$(git symbolic-ref -q --short refs/remotes/origin/HEAD 2>/dev/null) || padrao=""
    for b in $padrao origin/main origin/master main master; do
      if git rev-parse -q --verify "$b^{commit}" >/dev/null 2>&1; then
        base=$b
        break
      fi
    done
    [ -n "$base" ] || { echo "Não achei a base (origin/HEAD, main ou master): use --base." >&2; exit 1; }
  fi
  desde=$(git merge-base "$base" HEAD) || { echo "Sem ancestral comum entre $base e HEAD." >&2; exit 1; }
  # O formato que o PR-Agent lê, seja qual for a configuração do git de quem roda.
  git diff --no-color --no-ext-diff --src-prefix=a/ --dst-prefix=b/ "$desde" > "$diff" || exit 1
  onde=$arvore
  alvo="o ramo $(git rev-parse --abbrev-ref HEAD) contra $base"
fi
if [ ! -s "$diff" ]; then
  echo "Nada a revisar: nenhuma diferença em $alvo."
  exit 0
fi

md=$dir/resultado.md
json=$dir/resultado.json
set -- --diff-file "$diff" --output "$md"
[ "$comando" = review ] && set -- "$@" --json-output "$json"
set -- "$@" "$comando"
[ "$comando" = ask ] && set -- "$@" "$pergunta"

echo "PR-Agent: $comando de $alvo (claude-code/$modelo, esforço $esforco)" >&2
(
  cd "$onde" || exit 1
  CONFIG__MODEL="claude-code/$modelo" \
    CONFIG__FALLBACK_MODELS='[]' \
    CONFIG__PROPAGATE_TOOL_ERRORS=true \
    CONFIG__RESPONSE_LANGUAGE="${CONFIG__RESPONSE_LANGUAGE:-pt-BR}" \
    CONFIG__MAX_MODEL_TOKENS="${CONFIG__MAX_MODEL_TOKENS:-100000}" \
    CLAUDE_CODE__EFFORT="$esforco" \
    LOG_LEVEL="${LOG_LEVEL:-WARNING}" \
    exec "$bin" "$@"
) > "$dir/log" 2>&1
rc=$?
if [ "$rc" -ne 0 ] || [ ! -s "$md" ]; then
  esc=$(printf '\033')
  echo "O PR-Agent falhou (saída $rc). O fim do log ($dir/log):" >&2
  tail -n 25 "$dir/log" | sed "s/${esc}\[[0-9;]*m//g" >&2
  exit 1
fi
cat "$md"
echo
echo "---"
echo "Markdown: $md"
if [ -f "$json" ]; then
  echo "JSON: $json"
  sed -n '/"coverage"/,/^  }/p' "$json"
fi
echo "Log: $dir/log"
