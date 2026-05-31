#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="/tmp/network-service-features-target"
BP_SOURCE=""
CLEAN=""
MODEL=""
AGENT=""

usage() {
    echo "Usage: ./run.sh [--target <path>] [--bp-source <path>] [--model <model>] [--agent <agent>] [--clean]"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target) TARGET="$2"; shift 2 ;;
        --bp-source) BP_SOURCE="$2"; shift 2 ;;
        --model) MODEL="$2"; shift 2 ;;
        --agent) AGENT="$2"; shift 2 ;;
        --clean) CLEAN="--clean"; shift ;;
        --help|-h) usage ;;
        *) usage ;;
    esac
done

BP_SOURCE="${BP_SOURCE:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

if [[ ! -d "$TARGET/.claude/skills/service-architecture" ]] || [[ -n "$CLEAN" ]]; then
    "$SCRIPT_DIR/setup.sh" --target "$TARGET" --bp-source "$BP_SOURCE" $CLEAN
fi

LOG="/tmp/network-service-features-$(date +%Y%m%d-%H%M%S)"

PROMPT="This service was generated from the network-service battery pack and ships with companion skills. Use those skills as your starting point: they give breadcrumbs, not full recipes, so follow their pointers and read the referenced crates' own docs to fill in the details. Make three production-hardening changes, presenting complete code for each: (1) upgrade the rate limiter from a single global bucket to per-client (per-IP) limiting; (2) add a read-through cache in front of the downstream HTTP store; (3) add load shedding so the service returns 503 when in-flight concurrency exceeds a bound. For each change, state in one sentence the specific pitfall the skill flagged as easy to get wrong, and how your code avoids it. Do not add a cache to the in-memory store."

echo ""
echo "Running benchmark..."
echo "Target: $TARGET"
echo "Log: $LOG.md"
echo "---"
echo ""

START_TIME=$(date +%s)

EXTRA_FLAGS=""
[[ -n "$MODEL" ]] && EXTRA_FLAGS="$EXTRA_FLAGS --model $MODEL"
[[ -n "$AGENT" ]] && EXTRA_FLAGS="$EXTRA_FLAGS --agent $AGENT"

cd "$TARGET"
echo "$PROMPT" | claude -p --verbose --output-format stream-json \
    --allowed-tools "Read,Glob,Grep,Skill,Bash(cargo *)" \
    $EXTRA_FLAGS \
    | tee "$LOG.raw" \
    | jq -r --unbuffered 'select(.type == "assistant") | .message.content[]? | select(.type == "text" or .type == "thinking") | if .type == "thinking" then "<thinking>\n\(.thinking)\n</thinking>" else .text // empty end' \
    | tee "$LOG.md"

echo ""
echo "---"
echo "Output: $LOG.md"

END_TIME=$(date +%s)
echo "Duration: $((END_TIME - START_TIME))s"

jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | .input.skill' "$LOG.raw" > "$LOG.skills"
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use") | .name' "$LOG.raw" | sort | uniq -c | sort -rn > "$LOG.tools"

echo "Skills: $LOG.skills  Tools: $LOG.tools"
echo ""
echo "Evaluate with:"
echo "  Evaluate $LOG.md against $SCRIPT_DIR/EXPECTED.md"
