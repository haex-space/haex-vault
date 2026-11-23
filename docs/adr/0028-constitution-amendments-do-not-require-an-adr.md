# ADR 0028: Constitution amendments do not require a separate ADR

- **Status**: accepted
- **Date**: 2026-11-23
- **Amends**: the former `adrs-for-principle-affecting-decisions` constitution fragment

## Context

The constitution required every design decision that materially affects a Core Principle to be
recorded both in the constitution and in a separate ADR. Constitution amendments already update the
authoritative fragment and composed document, bump the constitution version, and are reviewed as a
change. Requiring a second document for every such amendment duplicates the decision and can leave
two accounts of the same rule to maintain.

## Decision

Remove the blanket requirement for an ADR when a decision materially affects a Core Principle.
Constitution amendments continue to follow the amendment procedure in the constitution. Authors may
add an ADR when it provides useful context or records alternatives, but the constitution does not
require one solely because a Core Principle changes.

## Consequences

- The authoritative principle and its change history remain in the constitution fragments, composed
  constitution, and reviewed commit history.
- An ADR can still document a decision when its rationale or rejected alternatives deserve a separate
  record.
- This ADR records the rationale for removing the former blanket requirement.
