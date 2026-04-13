# Lessons

> Mistakes made and the rules that prevent them. Review at session start. Add an entry every time the user corrects you.

## Format

Each entry:

**Mistake** — what went wrong
**Rule** — the invariant that prevents it
**Why** — the reason the rule exists (so edge cases can be judged, not blindly followed)

## Entries

### L1 — Stash-then-commit on a clean working tree silently no-ops
**Mistake**: Ran `git stash -u && git add ... && git commit ...` without first verifying the index actually had stageable changes for the target commit. The stash swept the staged rename into the stash, the commit found nothing, and the original edit had to be reconstructed.
**Rule**: Before stashing to isolate a commit, confirm that what you intend to commit is *not* part of what will be stashed. If the rename is staged and you stash everything, you stashed the rename too. Prefer `git commit <pathspec>` directly when only specific paths matter.
**Why**: `git stash -u` takes everything (staged, unstaged, untracked). Combining it with a follow-up commit assumes the index survives — it doesn't.
