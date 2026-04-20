## Tracking work-in-progress

The file WIP.md (when it exists) tracks our current ongoing task.

It is *very important* to keep it updated as you work so that we can remember what we have done and pick the work up again in a fresh session. Always edit the WIP.md file after completing work without waiting for confirmation from the user.

When we start a new session, read WIP.md (if it exists) to get oriented, but don't take action until I have confirmed what we should be working on.

## Snapbox assertions

When adding new screens or components, you must add snapbox snapshot tests using `snapbox::assert_data_eq!` with `file![_]` or `str![[...]]`. To generate or update snapshot files:

```
SNAPSHOTS=overwrite cargo nextest run --all-features
```