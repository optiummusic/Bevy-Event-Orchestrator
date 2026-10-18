# Releasing

Run commands from the directory containing this crate's `Cargo.toml`.
The current checkout places the crate in `bevy-event-orchestrator/` inside a
larger Git repository. Keep that path in mind when configuring CI or linking
source files. The `repository` metadata points to the configured Git remote.

## Validate

```sh
bash scripts/check.sh
cargo package --locked --list
cargo publish --locked --dry-run
```

The check script is suitable for CI. Configure the CI job at the actual Git
repository root, with Rust stable and the rustfmt/clippy components installed,
and invoke `bash bevy-event-orchestrator/scripts/check.sh` for this layout.
If the crate is moved to the repository root, invoke `bash scripts/check.sh`.

Test the declared minimum Rust version (1.97.1) separately before release if
that toolchain is available. The local preparation checks use the installed
stable toolchain; declaring a minimum does not itself verify that toolchain.

During local preparation with uncommitted work, use
`cargo package --locked --allow-dirty` to verify the archive. Inspect the file
list: it should contain source, examples, README, changelog, licenses and Cargo
metadata, without build output or local agent configuration. The `include`
allowlist in `Cargo.toml` controls the published files.

## Release

1. Confirm the package name is available on crates.io and that the repository
   URL is correct for the published source.
2. Update the version if needed, replace `Unreleased` in `CHANGELOG.md` with the
   release date, and check the README compatibility table.
3. Run validation, review the diff, and commit the release files.
4. Authenticate using `cargo login` in your own terminal if needed. Never put a
   registry token in source, documentation, or CI logs.
5. Run `cargo publish --locked --dry-run`, then `cargo publish --locked`.
6. Tag the released commit `v0.1.1` (or the selected version) and push the commit
   and tag. Create the repository release from the changelog.
7. Check the crates.io listing and the docs.rs build after publication.

No package upload, Git push, or release tag is performed by the preparation
script. Offline packaging verifies the archive locally; it does not establish
registry name availability or permission to publish.
