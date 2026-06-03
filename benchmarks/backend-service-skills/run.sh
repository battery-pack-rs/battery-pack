#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET="/tmp/backend-service-skills-target"
BP_SOURCE=""
CLEAN=""
MODEL=""
AGENT=""

usage() {
    echo "Usage: ./run.sh [--target <path>] [--bp-source <path>] [--model <model>] [--agent <agent>] [--clean]"
    echo ""
    echo "Options:"
    echo "  --target      Path to the generated service (default: /tmp/backend-service-skills-target)"
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

LOG="/tmp/backend-service-skills-$(date +%Y%m%d-%H%M%S)"

PROMPT="This project was generated from the backend-service battery pack. It ships companion skills, and because it depends on dial9 it also has dial9's own agent skills available. Work in two phases.

PHASE 1, bootstrap and observe. Build and run the service locally. IMPORTANT: run the server in the background with its output redirected to a log file (for example 'cargo run > server.log 2>&1 &') so it does not hold this session open, and kill it when you are done with it. Exercise each endpoint with curl: PUT and GET /items/{key}, POST /echo, GET /health. Confirm it emits structured logs and wide-event metrics. Then enable dial9 by setting its environment variables (see dial9.env) and confirm it writes trace files. Drive a brief burst of load against it (a loop of a few hundred curls, or 'oha' if available) so the trace reflects real activity, then use dial9's agent tooling (the 'dial9 agents' skills and toolkit) to analyze the trace and describe what the runtime was doing under load. Do NOT start 'dial9 serve' (it is a long-running web UI that will block this session); use the scripted agent analysis instead. Run the criterion benchmarks ('cargo bench') to completion. Report what you observed at each step, but do NOT draw performance conclusions; the goal is only to confirm the generated service bootstraps, runs, and is observable.

PHASE 2, layer on production features as follow-ups, using the service-architecture skill as your guide (the skill gives breadcrumbs, so read the referenced crates' docs to fill in details). The skill's 'cargo bp add backend-service -F <feature>' commands need '--path ${BP_SOURCE}/opinionated-battery-packs/backend-service-battery-pack' added in this benchmark because the pack is local rather than published; otherwise follow the skill as written. Make these changes and present the code for each: (1) upgrade the rate limiter from a global bucket to per-client (per-IP); (2) add a read-through cache in front of the HTTP forwarder; (3) add load shedding that returns 503 when in-flight concurrency exceeds a bound. For each, name the specific implementation footgun the skill flags (the easy-to-get-wrong part of wiring the feature, not the reason to use it) and show how your code avoids it. After implementing the three features, run the service with dial9 enabled again, drive another burst of load, and analyze the new trace, noting any effect the features have (rate-limit rejections, cache hits, shed responses).

When finished, make sure no server process you started is still running. Do not run a separate self-review pass or spawn another agent to review your work; just implement, confirm it builds and tests pass, and report."

# Renders text, thinking, bash commands ($ cmd), and other tool calls ([Name]) in order.
FILTER='select(.type == "assistant") | .message.content[]? | if .type == "thinking" then "<thinking>\n\(.thinking)\n</thinking>" elif .type == "text" then (.text // empty) elif .type == "tool_use" then (if .name == "Bash" then "$ \(.input.command)" elif .name == "Skill" then "[skill: \(.input.skill)]" else "[\(.name)]" end) else empty end'

echo ""
echo "Running benchmark (streaming below)..."
echo "Target: $TARGET"
echo "---"
echo ""

START_TIME=$(date +%s)

EXTRA_FLAGS=""
[[ -n "$MODEL" ]] && EXTRA_FLAGS="$EXTRA_FLAGS --model $MODEL"
[[ -n "$AGENT" ]] && EXTRA_FLAGS="$EXTRA_FLAGS --agent $AGENT"

# Stream live to the terminal (text, thinking, and commands) while capturing the raw stream.
cd "$TARGET"
echo "$PROMPT" | claude -p --verbose --output-format stream-json \
    --allowed-tools "Read,Glob,Grep,Skill,Edit,Write,Bash" \
    $EXTRA_FLAGS \
    | tee "$LOG.raw" \
    | jq -r --unbuffered "$FILTER"

DURATION=$(($(date +%s) - START_TIME))

# Assemble one self-contained, gist-ready report: summaries up top, raw stream at the bottom.
result_line() { jq -r 'select(.type == "result") | "Turns: \(.num_turns // "?")  Cost: $\(.total_cost_usd // "?")"' "$LOG.raw" 2>/dev/null | head -1; }

{
    echo "# Benchmark: backend-service-skills"
    echo
    echo "## Run"
    echo "- Date: $(date -Iseconds)"
    echo "- Model: ${MODEL:-default}"
    echo "- Agent: ${AGENT:-default}"
    echo "- Target: \`$TARGET\`"
    echo "- Duration: ${DURATION}s"
    echo "- $(result_line)"
    echo
    echo "## Skills invoked"
    jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | "- \(.input.skill)"' "$LOG.raw" | sort -u
    echo
    echo "## Bash commands"
    echo '```'
    jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Bash") | .input.command' "$LOG.raw"
    echo '```'
    echo
    echo "## Prompt"
    echo '```text'
    echo "$PROMPT"
    echo '```'
    echo
    echo "## Transcript"
    echo
    jq -r "$FILTER" "$LOG.raw"
    echo
    echo "---"
    echo
    echo "## Raw event stream"
    echo "<details><summary>full JSON stream</summary>"
    echo
    # 4-backtick fence so triple-backticks inside the stream do not close it.
    echo '````json'
    cat "$LOG.raw"
    echo '````'
    echo
    echo "</details>"
} > "$LOG.md"

echo ""
echo "---"
echo "Single gist-ready report: $LOG.md"
echo "Duration: ${DURATION}s"
echo ""
echo "Evaluate with:"
echo "  Evaluate $LOG.md against $SCRIPT_DIR/EXPECTED.md, then insert the findings as an '## Evaluation' section at the top of $LOG.md (just below the title)"

# Safety net: kill anything the agent left bound to the default dev port.
if command -v fuser >/dev/null 2>&1; then fuser -k 3000/tcp 2>/dev/null || true; fi
