# Development rules

- Keep canonical graph, view projection and output scene separate.
- Core modules must not depend on browser, Node or renderer APIs.
- Incremental execution and backend/frontend handoff are foundational requirements.
- Never apply stale layout results silently. Preserve logical edge direction.
- Support cycles, self-loops and parallel edges from the first functional milestone.
- Do not claim performance without reproducible measurements.
- Before copying third-party code, record its source revision, license and notices.
- Keep scaffold APIs explicitly provisional. Avoid unnecessary dependencies.
- Validate changes with formatting and appropriate focused checks.
