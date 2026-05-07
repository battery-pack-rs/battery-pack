#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="/tmp/error-skills-bench-target"
BP_SOURCE=""
CLEAN=""
MODEL=""
AGENT=""

usage() {
    echo "Usage: ./run.sh [--target <path>] [--bp-source <path>] [--model <model>] [--agent <agent>] [--clean]"
    echo ""
    echo "Options:"
    echo "  --target      Path to the test project (default: /tmp/error-skills-bench-target)"
    echo "  --bp-source   Path to the battery-pack repo (default: inferred from script location)"
    echo "  --model       Model to use (default: agent's configured default)"
    echo "  --agent       Agent to use (default: agent's configured default)"
    echo "  --clean       Reset the target project before setup"
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

# Setup if skills aren't installed or --clean
if [[ ! -d "$TARGET/.claude/skills/library-errors" ]] || [[ -n "$CLEAN" ]]; then
    "$SCRIPT_DIR/setup.sh" --target "$TARGET" --bp-source "$BP_SOURCE" $CLEAN
fi

LOG="/tmp/skill-benchmark-$(date +%Y%m%d-%H%M%S)"

PROMPT="Set up error handling for this mini-redis implementation following Rust best practices. The crate is a library with a public API (clients import it), so errors need to be opaque and evolvable. Internally there are multiple subsystems (connection handling, command parsing, storage) that should have their own error types. The binary should use anyhow for the application layer. Follow the guidance from the skills available in this project. Present the full implementation plan with complete code for each file in your response. Do not summarize, show all the code."

echo ""
echo "Running benchmark..."
echo "Target: $TARGET"
echo "Log: $LOG.md"
echo "---"
echo ""

START_TIME=$(date +%s)

MODEL_FLAG=""
if [[ -n "$MODEL" ]]; then
    MODEL_FLAG="--model $MODEL"
fi

EXTRA_FLAGS=""
[[ -n "$MODEL" ]] && EXTRA_FLAGS="$EXTRA_FLAGS --model $MODEL"
[[ -n "$AGENT" ]] && EXTRA_FLAGS="$EXTRA_FLAGS --agent $AGENT"

cd "$TARGET"
echo "$PROMPT" | claude -p --verbose --output-format stream-json \
    --allowed-tools "Read,Glob,Grep,Skill,Bash(cargo *)" \
    $EXTRA_FLAGS \
    $MODEL_FLAG \
    | tee "$LOG.raw" \
    | jq -r --unbuffered 'select(.type == "assistant") | .message.content[]? | select(.type == "text" or .type == "thinking") | if .type == "thinking" then "<thinking>\n\(.thinking)\n</thinking>" else .text // empty end' \
    | tee "$LOG.md"

echo ""
echo "---"
echo "Output: $LOG.md"
echo "Raw JSON: $LOG.raw"

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
echo "Duration: ${DURATION}s (started $(date -d @$START_TIME +%H:%M:%S), ended $(date -d @$END_TIME +%H:%M:%S))"

# Write run metadata
cat > "$LOG.meta" <<EOF
start: $(date -d @$START_TIME --iso-8601=seconds)
end: $(date -d @$END_TIME --iso-8601=seconds)
duration_s: $DURATION
target: $TARGET
model: ${MODEL:-default}
agent: ${AGENT:-default}
EOF

# Extract tool usage summary
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use") | .name' "$LOG.raw" \
    | sort | uniq -c | sort -rn > "$LOG.tools"

# Extract skills invoked
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | .input.skill' "$LOG.raw" \
    > "$LOG.skills"

# Extract all tool invocations with inputs
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use") | "\(.name): \(.input | tostring)"' "$LOG.raw" \
    > "$LOG.invocations"

# Extract internal reasoning
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "thinking") | .thinking' "$LOG.raw" \
    > "$LOG.thinking"

echo "Tools: $LOG.tools"
echo "Skills: $LOG.skills"
echo "Invocations: $LOG.invocations"
echo "Thinking: $LOG.thinking"
echo ""
echo "Evaluate with:"
echo "  Evaluate $LOG.md against $SCRIPT_DIR/EXPECTED.md"
