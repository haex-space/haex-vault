# Worktree convention

This directory contains git worktrees for in-flight refactors. Each subdirectory is checked out from a separate branch — list them with `git worktree list` rather than `ls`, since branches may be more or less stale than the file timestamps suggest.

- Do not delete worktrees blindly — check `git worktree list` to verify the branch is merged or abandoned first.
- New worktrees: `git worktree add .worktrees/<slug> <branch>`.
- Cleanup: `git worktree remove .worktrees/<slug>` (or `git worktree prune` for stale references).
