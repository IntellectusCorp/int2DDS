# Contributing to int2DDS

Thank you for your interest in contributing to int2DDS! We welcome contributions from the community.

int2DDS is an open-source real-time DDS middleware core.
All contributions are governed by the Apache License 2.0 and a Contributor License Agreement (CLA).

- License: Apache License 2.0 (see `LICENSE`)
- CLA:
  - Individual contributors: `CLA-Individual.md`
  - Corporate / organizational contributors: `CLA-Corporate.md`

**Contribution Acceptance Notice**

By submitting a pull request or otherwise contributing to this repository,
you agree to the terms of the applicable Contributor License Agreement (CLA).
No additional signature is required unless explicitly requested.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Environment](#development-environment)
- [Building and Testing](#building-and-testing)
- [Code Style Guidelines](#code-style-guidelines)
- [Commit Message Guidelines](#commit-message-guidelines)
- [Branch Naming Convention](#branch-naming-convention)
- [Pull Request Process](#pull-request-process)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Enhancements](#suggesting-enhancements)
- [Communication Channels](#communication-channels)

## Code of Conduct

This project adheres to a [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/int2DDS.git
   cd int2DDS
   ```
3. Add the upstream repository:
   ```bash
   git remote add upstream https://github.com/IntellectusCorp/int2DDS.git
   ```
4. Create a new branch for your changes (see [Branch Naming Convention](#branch-naming-convention))

## Development Environment

### Prerequisites

- **Rust**: 1.70 or later
  - Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Cargo**: Comes with Rust installation
- **Git**: For version control

### Recommended Tools

- **rust-analyzer**: LSP implementation for Rust (IDE support)
- **cargo-watch**: Auto-rebuild on file changes: `cargo install cargo-watch`
- **cargo-edit**: Manage dependencies from CLI: `cargo install cargo-edit`

### Setting Up Your Development Environment

```bash
# Clone the repository
git clone https://github.com/IntellectusCorp/int2DDS.git
cd int2DDS

# Build the project
cargo build

# Run tests to ensure everything works
cargo test

# Install development dependencies (optional)
cargo install cargo-watch cargo-edit
```

## Building and Testing

### Building

```bash
# Build in debug mode (faster compilation, slower runtime)
cargo build

# Build in release mode (slower compilation, optimized runtime)
cargo build --release

# Build specific package
cargo build -p int2dds
cargo build -p int2dds-derive
cargo build -p int2dds-ffi

# Build with all features
cargo build --all-features
```

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific package
cargo test -p int2dds

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests with logging enabled
RUST_LOG=debug cargo test
```

### Running Examples

```bash
# List the core hello_world examples
ls dds/examples/

# Run specific example
cargo run --example hello_world_pub -- --domain 0
cargo run --example hello_world_sub -- --domain 0

# Run with environment variables
INT2DDS_THREAD_MONITORING=true cargo run --example hello_world_pub -- --domain 0
RUST_LOG=info cargo run --example hello_world_sub -- --domain 0
```

### Linting and Formatting

```bash
# Format code (automatically fixes formatting issues)
cargo fmt

# Check formatting without making changes
cargo fmt -- --check

# Run Clippy (linter)
cargo clippy

# Run Clippy with all features and strict warnings
cargo clippy --all-features -- -D warnings
```

## Code Style Guidelines

### General Principles

- **Follow Rust conventions**: Use `rustfmt` and `clippy`
- **Write safe code**: Prefer safe Rust over unsafe when possible
- **Document public APIs**: All public items should have documentation comments
- **Write tests**: Add tests for new functionality

### Formatting Rules

The project uses `rustfmt` with custom configuration (see `rustfmt.toml`):

- **Line width**: 100 characters maximum
- **Indentation**: 4 spaces (no hard tabs)
- **Import organization**: Sorted and grouped
- **Trailing commas**: Required in multi-line expressions

### Naming Conventions

- **Types**: `PascalCase` (e.g., `DomainParticipant`)
- **Functions/methods**: `snake_case` (e.g., `create_topic`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_TIMEOUT`)
- **Modules**: `snake_case` (e.g., `rtps_message`)

### Documentation

- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Include examples in documentation where appropriate
- Follow the [Rust documentation guidelines](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)

Example:

````rust
/// Creates a new DomainParticipant.
///
/// # Arguments
///
/// * `domain_id` - The domain ID to join
///
/// # Errors
///
/// Returns an error if the participant cannot be created.
///
/// # Example
///
/// ```
/// use int2dds::dds::domain::DomainParticipant;
///
/// let participant = DomainParticipant::new(0)?;
/// ```
pub fn new(domain_id: u32) -> Result<Self, Error> {
    // Implementation
}
````

### Error Handling

- Use `Result<T, E>` for fallible operations
- Provide meaningful error messages
- Use custom error types when appropriate
- Don't panic in library code unless it's truly unrecoverable

## Commit Message Guidelines

We follow a standardized commit message format to maintain a clear history.

### Format

```
TYPE: Brief description (max 72 characters)

Optional detailed description explaining the changes.
Can span multiple lines.

Fixes #123
```

### Commit Types

- `FEAT`: New features or functionality
- `FIX`: Bug fixes
- `DOCS`: Documentation changes only
- `STYLE`: Code style/formatting changes (no logic changes)
- `REFACTOR`: Code refactoring (no functional changes)
- `TEST`: Adding or modifying tests
- `CHORE`: Build process, tooling, dependencies
- `PERF`: Performance improvements

### Examples

```bash
# Good commit messages
FEAT: Add support for TCP transport in RTPS layer
FIX: Resolve memory leak in SendingTask thread pool
DOCS: Update README with installation instructions
REFACTOR: Simplify QoS policy matching logic
TEST: Add integration tests for DataWriter
CHORE: Update dependencies to latest versions
```

### Guidelines

- Use imperative mood ("Add feature" not "Added feature")
- First line should be concise (max 72 characters)
- Separate subject from body with a blank line
- Reference related issues with `Fixes #issue_number`
- Explain **what** and **why**, not **how** (the code shows how)

## Branch Naming Convention

Use the following prefixes for branch names:

- `feature/` - New features
  - Example: `feature/tcp-transport`
- `fix/` - Bug fixes
  - Example: `fix/memory-leak-sending-task`
- `hotfix/` - Urgent production fixes
  - Example: `hotfix/critical-crash`
- `docs/` - Documentation improvements
  - Example: `docs/contributing-guide`
- `refactor/` - Code refactoring
  - Example: `refactor/qos-policy-matching`
- `test/` - Test additions or improvements
  - Example: `test/integration-tests`

**Branch naming rules:**

- Use hyphens (`-`), not underscores (`_`)
- Use lowercase letters
- Be descriptive but concise
- Good: `feature/websocket-transport`
- Bad: `my_feature`, `fix1`, `WIP`

## Pull Request Process

### Before Submitting

1. **Update your branch** with the latest upstream changes:

   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Ensure all tests pass**:

   ```bash
   cargo test
   ```

3. **Run code formatting**:

   ```bash
   cargo fmt
   ```

4. **Run linter**:

   ```bash
   cargo clippy -- -D warnings
   ```

5. **Update documentation** if you changed public APIs

6. **Add tests** for new functionality

### Submitting a Pull Request

1. Push your branch to your fork:

   ```bash
   git push origin feature/your-feature-name
   ```

2. Go to the [int2DDS repository](https://github.com/IntellectusCorp/int2DDS) and click "New Pull Request"

3. Select your branch and provide a clear description:

   - **Title**: Brief description of changes
   - **Summary**: What does this PR do?
   - **Rationale**: Why is this change needed?
   - **Testing**: How was this tested?
   - **Related Issues**: Link to related issues (e.g., "Fixes #123")

4. Use the provided [Pull Request Template](.github/PULL_REQUEST_TEMPLATE.md)

5. Request review from maintainers

### During Review

- Respond to feedback promptly
- Make requested changes in new commits (don't force-push)
- Mark conversations as resolved when addressed
- Be respectful and professional

### After Approval

- Maintainers will merge your PR
- Your branch will be deleted (you can delete your local branch too)
- Celebrate your contribution!

## Reporting Bugs

### Before Submitting a Bug Report

- Check the [existing issues](https://github.com/IntellectusCorp/int2DDS/issues) to avoid duplicates
- Ensure you're using the latest version
- Try to reproduce the issue with minimal code

### Submitting a Bug Report

Use the [Bug Report Template](.github/ISSUE_TEMPLATE/bug_report.md) and include:

- **Description**: Clear description of the bug
- **Steps to Reproduce**: Minimal steps to reproduce the issue
- **Expected Behavior**: What you expected to happen
- **Actual Behavior**: What actually happened
- **Environment**:
  - OS and version (Windows, Linux, macOS)
  - Rust version (`rustc --version`)
  - int2DDS version
- **Code Sample**: Minimal code that reproduces the issue
- **Logs**: Relevant log output (use `RUST_LOG=debug`)

## Suggesting Enhancements

### Before Submitting an Enhancement

- Check if the feature already exists
- Search [existing issues](https://github.com/IntellectusCorp/int2DDS/issues) for similar suggestions
- Consider if it fits the project's scope

### Submitting an Enhancement Request

Use the [Enhancement Request Template](.github/ISSUE_TEMPLATE/enhancement_request.md) and include:

- **Problem**: What problem does this solve?
- **Proposed Solution**: How would you implement this?
- **Alternatives**: What alternatives have you considered?
- **Use Case**: Real-world scenarios where this would be useful
- **Benefits**: How does this improve int2DDS?

## Communication Channels

- **GitHub Issues**: Bug reports and feature requests
- **Pull Requests**: Code contributions and reviews
- **Discussions**: For questions, ideas, and general discussion (if enabled)

## Development Workflow Example

Here's a example workflow:

```bash
# 1. Sync with upstream
git checkout main
git fetch upstream
git merge upstream/main

# 2. Create a new branch
git checkout -b feature/my-awesome-feature

# 3. Make changes and commit
# ... edit files ...
cargo fmt
cargo clippy
cargo test
git add .
git commit -m "FEAT: Add my awesome feature"

# 4. Push to your fork
git push origin feature/my-awesome-feature

# 5. Create Pull Request on GitHub

# 6. Address review feedback
# ... make changes ...
git add .
git commit -m "REFACTOR: Address review feedback"
git push origin feature/my-awesome-feature

# 7. After merge, clean up
git checkout main
git pull upstream main
git branch -d feature/my-awesome-feature
```

## License

By contributing to int2DDS, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).

---

Thank you for contributing to int2DDS! Your efforts help make this project better for everyone.
