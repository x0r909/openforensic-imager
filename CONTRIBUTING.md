# Contributing to OpenForensic Imager

Thank you for considering a contribution. This document states how to work with the project, what a change must satisfy before it is merged, and how security is reported.

OpenForensic Imager is forensic software. The bar here is deliberate: a change must not compromise evidence integrity, the sandbox, or the reproducibility of a release.

## Code of conduct

The project follows the [Contributor Covenant, version 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/). All maintainers, contributors, and users in the project's spaces are expected to adhere to it.

## Licensing

All new code is contributed under the project license, GPL-3.0-or-later. By submitting a pull request you agree to license your contribution under that license. If you contribute code you do not hold the rights to, or code with an incompatible license, disclose that in the pull request before it is merged.

## Before you start

Open an issue to describe the change before writing code, unless the change is a trivial fix. State the problem, the evidence, and the intended behavior. This avoids duplicate work and lets maintainers flag scope or design issues early.

Small, focused pull requests are preferred. One logical change per pull request.

## Branching and workflow

- Work on a branch created from `main`. Use a descriptive name, for example `fix/worker-canonicalize` or `test/ewf-relative-path`.
- Open a pull request against `main`.
- A pull request is ready for review only when all checks pass, all tests are green, and the change has a clear description of what it does, why, and how it was verified.
- Maintainers squash-merge. Commit history on `main` is kept linear.

## Coding standards

- Rust code must build cleanly with `cargo clippy --workspace --all-targets -- -D warnings`.
- The project pins the Rust toolchain in `rust-toolchain.toml` (currently 1.93.1). Build against the pinned toolchain.
- Name functions, types, and crates to match existing conventions. Follow the naming style already present in the crate you touch.
- Every public function and non-trivial internal function has a doc comment explaining its contract, including error behavior.
- Do not use `unsafe` where a safe alternative exists. Where `unsafe` is unavoidable, document why, and keep it isolated.
- Do not add new external dependencies without discussing them in the issue or pull request first. New native dependencies in particular affect packaging, reproducibility, and license compliance.
- Evidence-handling code must never silently swallow errors. Errors propagate to the caller with an error code and a message.
- Keep changes minimal and scoped. Do not reformat unrelated code.

## Testing requirements

Every behavioral change must include tests.

- Unit tests live next to the code they cover and use existing test conventions.
- Integration tests live in the `tests/` directory of the affected crate.
- Run the full workspace suite before opening a pull request:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- If your change touches sandboxing, acquisition, or filesystem parsing, also run the relevant sanitizer and fuzz budget scripts if feasible: `scripts/run-sanitizers.sh` and `scripts/fuzz-budget.sh`.
- Tests must be deterministic and must not depend on network access or on absolute host paths.
- A change that alters output formats, hashes, or behavior must update the relevant documentation and per-phase result documents.

## Evidence integrity

- Code that reads, writes, or verifies evidence must preserve the read-only guarantee for source images. A change that relaxes any integrity control must be flagged in the pull request and justified in the issue.
- The worker sandbox is a security boundary. Changes to `crates/worker-sandbox`, `crates/native-bindings/src/parser_worker.rs`, or the Landlock and seccomp policies require a security-focused review and tests demonstrating the new boundary.
- Do not commit sample images, case data, logs, or `packaging/output/` artifacts to the repository.

## Commit messages

- Use Conventional Commits: `feat(scope):`, `fix(scope):`, `docs:`, `test(scope):`, `refactor(scope):`, or `chore:`, with a scope that names the affected crate or area.
- The body states what changed and why. Reference the issue number when applicable.
- Use punctuation and sentence case. Do not end the subject line with a period.

## Security reporting

Do not open a public issue for a suspected vulnerability. Report it privately to the maintainers by direct email. Include a description of the flaw, the affected version, and reproduction steps. Maintainers will acknowledge receipt and coordinate a fix and disclosure. The threat model and the security controls the project enforces are described in [SECURITY_MODEL.md](SECURITY_MODEL.md).

## Review process

- Maintainers review for correctness, integrity, test coverage, and scope.
- Address review feedback by pushing new commits or amending within your branch. Do not force-push a reviewed branch without coordination.
- A pull request is merged after at least one maintainer approval and all checks pass.
- Maintainers may request changes for missing tests, unclear behavior, or scope creep. Those are normal parts of the process.

## Getting help

Ask questions in the issue tracker. For design discussions, open an issue and tag it `design`.
