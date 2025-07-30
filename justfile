# Every commit on the master branch is expected to have working `check` and `test-*` recipes.
#
# The recipes make heavy use of `rustup`'s toolchain syntax (e.g. `cargo +nightly`). `rustup` is
# required on the system in order to intercept the `cargo` commands and to install and use the appropriate toolchain with components. 

NIGHTLY_TOOLCHAIN := "nightly-2025-07-10"
STABLE_TOOLCHAIN := "1.88.0"

@_default:
    just --list

# Quick check including lints and formatting. Run "fix" mode for auto-fixes.
@check mode="verify":
  # Use nightly toolchain for modern format and lint rules.
  # Ensure the toolchain is installed and has the necessary components.
  rustup component add --toolchain {{NIGHTLY_TOOLCHAIN}} rustfmt clippy
  just _check-{{mode}}

# Verify check, fails if anything is off. Good for CI.
@_check-verify:
  # Cargo's wrapper for rustfmt predates workspaces, so uses the "--all" flag instead of "--workspaces".
  cargo +{{NIGHTLY_TOOLCHAIN}} fmt --check --all
  # Lint all workspace members. Enable all feature flags. Check all targets (tests, examples) along with library code. Turn warnings into errors.
  cargo +{{NIGHTLY_TOOLCHAIN}} clippy --all-features --all-targets -- -D warnings
  # Static analysis of types and lifetimes.
  # Nightly toolchain required by benches target.
  cargo +{{NIGHTLY_TOOLCHAIN}} check --all-features --all-targets
  # Build documentation to catch any broken doc links or invalid rustdoc.
  RUSTDOCFLAGS="-D warnings" cargo +{{STABLE_TOOLCHAIN}} doc --all-features --no-deps

# Attempt any auto-fixes for format and lints.
@_check-fix:
  # No --check flag to actually apply formatting.
  cargo +{{NIGHTLY_TOOLCHAIN}} fmt --all
  # Adding --fix flag to apply suggestions with --allow-dirty.
  cargo +{{NIGHTLY_TOOLCHAIN}} clippy --all-features --all-targets --fix --allow-dirty -- -D warnings

# Run a test suite: features, msrv, constraints, no-std, or all.
@test suite="features":
  just _test-{{suite}}

# Run all test suites.
@_test-all: _test-features _test-msrv _test-constraints

# Test library with feature flag matrix compatability.
@_test-features:
  # Test the extremes: all features enabled as well as none. If features are additive, this should expose conflicts.
  # If non-additive features (mutually exclusive) are defined, more specific commands are required.
  # Run all targets except benches which needs the nightly toolchain.
  cargo +{{STABLE_TOOLCHAIN}} test --no-default-features --lib --bins --tests --examples
  cargo +{{STABLE_TOOLCHAIN}} test --all-features --lib --bins --tests --examples
  cargo +{{STABLE_TOOLCHAIN}} test --all-features --doc

# Check code with MSRV compiler.
@_test-msrv:
  # Handles creating sandboxed environments to ensure no newer binaries sneak in.
  cargo install cargo-msrv@0.18.4
  cargo msrv --manifest-path protocol/Cargo.toml verify --all-features
  cargo msrv --manifest-path traffic/Cargo.toml verify --all-features

# Check minimum and maximum dependency contraints.
@_test-constraints:
  # Ensure that the workspace code works with dependency versions at both extremes. This checks
  # that we are not unintentionally using new feautures of a dependency or removed ones.
  # Skipping "--all-targets" for these checks since tests and examples are not relevant for a library consumer.
  # Enabling "--all-features" so all dependencies are checked.
  # Clear any previously resolved versions and re-resolve to the minimums.
  rm -f Cargo.lock
  cargo +{{NIGHTLY_TOOLCHAIN}} check --all-features -Z direct-minimal-versions
  # Clear again and check the maximums by ignoring any rust-version caps. 
  rm -f Cargo.lock
  cargo +{{NIGHTLY_TOOLCHAIN}} check --all-features --ignore-rust-version
  rm -f Cargo.lock

# Publish a new version.
@publish version remote="upstream":
  # Requires write privileges on upsream repository.
   
  # Publish guardrails: be on a clean master, updated changelog, updated manifest.
  if ! git diff --quiet || ! git diff --cached --quiet; then \
    echo "publish: Uncommitted changes"; exit 1; fi
  if [ "`git rev-parse --abbrev-ref HEAD`" != "master" ]; then \
    echo "publish: Not on master branch"; exit 1; fi
  if ! grep -q "## v{{version}}" CHANGELOG.md; then \
    echo "publish: CHANGELOG.md entry missing for v{{version}}"; exit 1; fi
  if ! grep -q '^version = "{{version}}"' Cargo.toml; then \
    echo "publish: Cargo.toml version mismatch"; exit 1; fi
  # Final confirmation, exit 1 is used to kill the script.
  printf "Publishing v{{version}}, do you want to continue? [y/N]: "; \
  read response; \
  case "$response" in \
    [yY]) ;; \
    *) echo "publish: Cancelled"; exit 1 ;; \
  esac
  # Publish the tag.
  echo "publish: Adding release tag v{{version}} and pushing to {{remote}}..."
  # Using "-a" annotated tag over a lightweight tag for robust history.
  git tag -a v{{version}} -m "Release v{{version}}"
  git push {{remote}} v{{version}}
