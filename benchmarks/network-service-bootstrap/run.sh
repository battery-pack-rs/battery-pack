#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="/tmp/network-service-bootstrap-target"
BP_SOURCE=""
CLEAN=""
MODEL=""
AGENT=""

usage() {
    echo "Usage: ./run.sh [--target <path>] [--bp-source <path>] [--model <model>] [--agent <agent>] [--clean]"
    echo ""
    echo "Options:"
    echo "  --target      Path to the generated service (default: /tmp/network-service-bootstrap-target)"
    echo "  --bp-source   Path to the battery-pack repo (default: inferred from script location)"
    echo "  --model       Model to use (default: agent's configured default)"
    echo "  --agent       Agent to use (default: agent's configured default)"
    echo "  --clean       Regenerate the target project before setup"
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

if [[ ! -d "$TARGET/.claude/skills/telemetry" ]] || [[ -n "$CLEAN" ]]; then
    "$SCRIPT_DIR/setup.sh" --target "$TARGET" --bp-source "$BP_SOURCE" $CLEAN
fi

LOG="/tmp/network-service-bootstrap-$(date +%Y%m%d-%H%M%S)"

PROMPT="This project was generated from the network-service battery pack. Bootstrap it end to end and confirm it works. Build it, run the server locally, and use curl to exercise each endpoint: GET and PUT on /items/{key}, POST /echo, and GET /health. Confirm it emits metrics and structured logs. Then enable dial9 runtime profiling through its environment variables and confirm a trace is produced, and run the criterion benchmarks to completion. Use the skills available in this project to understand the telemetry wiring and how to turn on dial9. Report what you observed at each step. Do NOT draw performance conclusions or quote numbers as results; the only goal is to confirm the scaffold bootstraps, runs, and is observable end to end."

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
    --allowed-tools "Read,Glob,Grep,Skill,Bash(cargo *),Bash(curl *),Bash(DIAL9_*),Bash(kill *),Bash(ls *),Bash(cat *)" \
    $EXTRA_FLAGS \
    | tee "$LOG.raw" \
    | jq -r --unbuffered 'select(.type == "assistant") | .message.content[]? | select(.type == "text" or .type == "thinking") | if .type == "thinking" then "<thinking>\n\(.thinking)\n</thinking>" else .text // empty end' \
    | tee "$LOG.md"

echo ""
echo "---"
echo "Output: $LOG.md"

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
echo "Duration: ${DURATION}s"

jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use") | .name' "$LOG.raw" \
    | sort | uniq -c | sort -rn > "$LOG.tools"
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | .input.skill' "$LOG.raw" \
    > "$LOG.skills"
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Bash") | .input.command' "$LOG.raw" \
    > "$LOG.commands"

echo "Tools: $LOG.tools  Skills: $LOG.skills  Commands: $LOG.commands"
echo ""
echo "Evaluate with:"
echo "  Evaluate $LOG.md against $SCRIPT_DIR/EXPECTED.md"
