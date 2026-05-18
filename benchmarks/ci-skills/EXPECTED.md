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

- [ ] `github-ci-fundamentals` appears in init skills list
- [ ] `trusted-publishing` appears in init skills list
- [ ] `binary-releases` appears in init skills list
- [ ] Agent invoked `github-ci-fundamentals` skill
- [ ] Agent invoked `trusted-publishing` skill
- [ ] Agent invoked `binary-releases` skill

## CI fundamentals (from github-ci-fundamentals skill)

- [ ] Mentions the gate job pattern (`ci-pass` as single required status check)
- [ ] Lists core CI jobs (fmt, clippy, build matrix, MSRV, semver-checks, etc.)
- [ ] Mentions SHA pinning for GitHub Actions
- [ ] References Dependabot for keeping actions and deps updated
- [ ] Mentions concurrency groups for PR workflows
- [ ] References `cargo bp add ci -t <template>` or `cargo bp new ci` as the generation command

## Trusted publishing (from trusted-publishing skill)

- [ ] Explains the two-job flow (release then release-pr)
- [ ] Instructs to configure trusted publishing on crates.io (OIDC, no API token)
- [ ] Mentions enabling "Allow GitHub Actions to create and approve pull requests"
- [ ] Explains PAT requirement for binary releases (GITHUB_TOKEN doesn't trigger other workflows)
- [ ] Specifies PAT permissions needed (contents:write, pull-requests:write)
- [ ] Mentions `RELEASE_PLZ_TOKEN` as the secret name
- [ ] References `release-plz.toml` configuration

## Binary releases (from binary-releases skill)

- [ ] Describes the build matrix (linux x86/arm, macOS x86/arm, windows)
- [ ] Mentions cargo-binstall discovery via `[package.metadata.binstall]`
- [ ] Explains the trigger chain (release-plz creates release, triggers binary build)
- [ ] Mentions the `[[bin]]` section requirement in Cargo.toml
- [ ] References version extraction from release tags

## Setup instructions completeness

- [ ] Covers crates.io trusted publisher registration
- [ ] Covers GitHub repo settings (Actions permissions)
- [ ] Covers PAT creation and secret setup
- [ ] Covers branch protection (ci-pass as required check)
- [ ] Presents steps in a logical order (generate first, then configure services)

## Anti-patterns to flag

- [ ] Does NOT suggest storing crates.io API tokens as secrets (should use OIDC)
- [ ] Does NOT suggest using `GITHUB_TOKEN` for the release job when binary releases are enabled
- [ ] Does NOT omit the manual setup steps (crates.io, GitHub settings)
- [ ] Does NOT present workflows without SHA-pinned actions
