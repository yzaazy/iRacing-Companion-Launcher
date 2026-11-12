---
name: claim
description: Claim a GitHub issue before working on it, and release the claim when dropping it. Use when picking up an issue ("work on #N", "fix issue N", an issue link), before creating its branch, writing code or starting a delegate for it.
---

## Facts

- Repository: `yzaazy/iRacing-Companion-Launcher`. Every session and agent works through the owner's `gh` account, so the assignee alone cannot tell two sessions apart. A claim is three marks together: the owner as assignee, the label `in progress`, and one comment naming the branch.
- Picking up an issue at the owner's request is the go for claiming it and for releasing that claim. Pushing the issue's branch and opening its pull request need no go either (`main` is protected and only the owner merges); a tag or a release still does.

## Claim

1. **Read** the issue:
   ```powershell
   gh issue view N --json state,assignees,labels,comments
   gh api repos/yzaazy/iRacing-Companion-Launcher/issues/N --jq '.type.name'
   ```
   An issue without a label (`bug`, `enhancement`, `documentation`) or a type (`Bug`, `Feature`, `Task`) gets them now: `gh issue edit N --add-label <label>`, `gh api -X PATCH repos/yzaazy/iRacing-Companion-Launcher/issues/N -f type=<Type>`.
2. **Stop** and tell the owner, without claiming, when the issue is closed, or when it is already claimed: it has the label `in progress`, or its latest `Claimed:` comment has no `Released:` comment after it. Name the branch the claim gives. Go on only when the owner says so (for example to take over that branch).
3. **Worktree**: the issue gets its own worktree on branch `issue-N-<short-slug>` from `origin/main`, because several sessions share the main checkout and a branch switch there moves every one of them:
   ```powershell
   git fetch
   git worktree add .claude/worktrees/issue-N -b issue-N-<slug> origin/main
   git -C .claude/worktrees/issue-N branch --unset-upstream
   ```
   From here on every edit, gate run, commit and push happens inside `.claude/worktrees/issue-N`; the main checkout stays on the branch it is on. When the session already runs in a worktree of its own (its top level, `git rev-parse --show-toplevel`, is not the main checkout), make the branch there instead: `git switch -c issue-N-<slug> origin/main`, then `git branch --unset-upstream`.
4. **Claim**:
   ```powershell
   gh issue edit N --add-assignee "@me" --add-label "in progress"
   gh issue comment N --body 'Claimed: working on this on branch `issue-N-<slug>`.'
   ```
   When the label does not exist yet, create it first:
   ```powershell
   gh label create "in progress" --color FBCA04 --description "Claimed: work on this has started"
   ```
5. The pull request for the issue says `Closes #N` in its body, so merging it closes the issue.

## Release

When the work is dropped, handed back unfinished or moved to another issue:

```powershell
gh issue edit N --remove-label "in progress" --remove-assignee "@me"
gh issue comment N --body 'Released: <why, and what state the branch is in>.'
```

When the pull request is merged the issue closes by itself; then remove only the label (`gh issue edit N --remove-label "in progress"`), with no comment, and the worktree (`git worktree remove .claude/worktrees/issue-N`).
