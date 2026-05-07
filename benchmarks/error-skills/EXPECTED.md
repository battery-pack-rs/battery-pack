# Expected Outcomes

## Tool usage analysis (run against `$LOG.raw`)

```bash
# Skills loaded and invoked
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Skill") | .input.skill' "$LOG.raw"

# All tool calls (summary of what the agent did)
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use") | .name' "$LOG.raw" | sort | uniq -c | sort -rn

# Files read (codebase exploration)
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Read") | .input.file_path // .input.path // empty' "$LOG.raw"

# Bash commands run
jq -r 'select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "Bash") | .input.command' "$LOG.raw"

# Total turns
jq -r 'select(.type == "result") | "Turns: \(.num_turns), Cost: $\(.total_cost_usd)"' "$LOG.raw"
```

## Skill activation

- [ ] `error-handling-basics` appears in init skills list
- [ ] `library-errors` appears in init skills list
- [ ] `application-errors` appears in init skills list
- [ ] Agent invoked `library-errors` skill
- [ ] Agent invoked `application-errors` skill
- [ ] Agent read project source files before proposing changes

## Library error types (from library-errors skill)

- [ ] Internal errors use `thiserror` with `pub(crate)` visibility
- [ ] Separate error enums per subsystem (connection, parsing, storage)
- [ ] Internal errors use `#[from]` for cross-module conversions
- [ ] Public error type uses Error+ErrorKind pattern OR thiserror with boxed sources
- [ ] Public error kind enum is `#[non_exhaustive]`
- [ ] Public error struct has private fields with accessor methods
- [ ] `From` impls convert internal errors to public error kinds
- [ ] `Display` does not include the source message
- [ ] Error messages are lowercase, no trailing punctuation
- [ ] `source()` is implemented and returns the underlying cause

## Application layer (from application-errors skill)

- [ ] Binary uses `anyhow::Result` (either via `main()` return or explicit handling)
- [ ] Context is added with `.context()` describing what was attempted
- [ ] No `unwrap()` in non-test code

## Anti-patterns to flag

- [ ] Does NOT expose dependency types in public error variants
- [ ] Does NOT include source in Display format strings
- [ ] Does NOT use a single god-error enum for the whole crate
