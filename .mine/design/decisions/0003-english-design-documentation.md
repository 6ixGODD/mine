# ADR-0003: Write the Design Knowledge Base in English

## Status

Accepted.

## Decision

All `docs/design/` content is written in English. User-facing documentation is
bilingual: English is the canonical source, with a Simplified Chinese version
semantically aligned (never a word-for-word translation) for
`docs/user-guide*`, `docs/concepts*`, and the root READMEs. The repository
root provides an English README and a Chinese translation. Troubleshooting
content, when present, is folded into the user guide rather than kept as a
separate document.

## Rationale

English aligns with code, protocols, upstream documentation, and the user's learning goal. One language inside the design tree avoids duplicate documents drifting apart. User-facing bilingual documentation makes MINE usable without reading internal Design.

## Consequences

- `docs/design/` remains English-only; user-facing bilingual documentation
  lives at `docs/user-guide*`, `docs/concepts*`, and root READMEs;
- troubleshooting guidance lives inside `docs/user-guide*` (no separate
  `docs/troubleshooting*` document);
- identifiers and technical contracts remain English;
- Skills may converse in the user's language while writing repository docs in English;
- Chinese user-facing documents must stay semantically aligned with the
  English canonical versions.
