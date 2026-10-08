# Project conventions

## Language
Use English throughout the codebase, including identifiers, comments,
documentation, and commit messages.

## Validation
The user performs manual testing and provides feedback.
Do not write or run automated tests unless explicitly requested.
Implement fixes based on the user's feedback.

## Commits
Use Conventional Commits: `<type>(<optional scope>): <description>`.
Examples:
- `feat(conversion): add image profiles`
- `fix(shell): refresh context menu profiles`
- `docs: update project documentation`

Create commits only when explicitly requested.

## FFmpeg
Use the latest stable FFmpeg release available when integrating or updating
the conversion engine. Bundle matching versions of ffmpeg and ffprobe,
and pin the exact version for each application release.
