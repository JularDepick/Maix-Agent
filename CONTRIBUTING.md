# Contributing to Maix-Agent

Thank you for your interest in contributing to Maix-Agent! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Documentation](#documentation)
- [Issue Reporting](#issue-reporting)
- [License](#license)

## Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Getting Started

### Prerequisites

- Rust 1.70+ (stable)
- Git
- SQLite (for development)

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/your-username/Maix-Agent.git
   cd Maix-Agent
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/JularDepick/Maix-Agent.git
   ```

## Development Setup

### Build the Project

```bash
# Debug build
cargo build --workspace

# Release build
cargo build --workspace --release

# Run tests
cargo test --workspace
```

### Configuration

1. Copy the default configuration:
   ```bash
   cp config/default.toml config/local.toml
   ```
2. Edit `config/local.toml` with your settings
3. Set environment variables for API keys:
   ```bash
   export ANTHROPIC_API_KEY="your-key"
   export OPENAI_API_KEY="your-key"
   ```

## Making Changes

### Branch Naming

- Feature: `feature/your-feature-name`
- Bug fix: `fix/issue-number-description`
- Documentation: `docs/what-you-changed`
- Refactor: `refactor/what-you-refactored`

### Commit Messages

Follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

Examples:
```
feat(memory): add semantic search capabilities
fix(tui): resolve input cursor positioning issue
docs(readme): update installation instructions
```

### Code Organization

- **crates/maix-core**: Core types, configuration, and utilities
- **crates/maix-db**: Database operations
- **crates/maix-provider**: LLM provider integrations
- **crates/maix-tools**: Tool implementations
- **crates/maix-memory**: Memory system
- **crates/maix-agent**: Agent runtime and orchestration
- **crates/maix-server**: Server implementation
- **crates/maix-cli**: CLI client
- **crates/maix-tui**: Terminal UI
- **crates/maix-gateway**: HTTP gateway

## Pull Request Process

1. **Update Documentation**: Ensure any new features are documented
2. **Add Tests**: Include tests for new functionality
3. **Run Checks**: Ensure all checks pass:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
4. **Update Changelog**: Add an entry to `CHANGELOG.md`
5. **Create PR**: Use the PR template and fill in all sections
6. **Review**: Address any review comments
7. **Merge**: Once approved, your PR will be merged

### PR Template

```markdown
## Description

Brief description of the changes.

## Type of Change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update

## Testing

Describe the tests you ran and how to reproduce them.

## Checklist

- [ ] My code follows the project's coding standards
- [ ] I have performed a self-review of my code
- [ ] I have added tests that prove my fix is effective or my feature works
- [ ] New and existing unit tests pass locally with my changes
- [ ] I have updated the documentation accordingly
```

## Coding Standards

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting
- Use `clippy` for linting
- Prefer `Result<T, E>` over panics
- Use meaningful variable and function names
- Add documentation comments for public APIs

### Error Handling

```rust
// Good
fn process_data(data: &str) -> Result<ProcessedData, Error> {
    // Implementation
}

// Bad
fn process_data(data: &str) -> ProcessedData {
    // Implementation with unwrap()
}
```

### Documentation

- Use `///` for public API documentation
- Include examples in documentation
- Document panics and errors
- Document safety considerations for unsafe code

### Testing

- Write unit tests for all new functionality
- Use descriptive test names
- Test edge cases and error conditions
- Use property-based testing where appropriate

## Testing

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run specific test
cargo test -p maix-core test_name

# Run with output
cargo test --workspace -- --nocapture
```

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Arrange
        let input = "test";
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn test_error_case() {
        let result = function_under_test("invalid");
        assert!(result.is_err());
    }
}
```

## Documentation

### Building Documentation

```bash
# Build and open documentation
cargo doc --workspace --open

# Check documentation
cargo doc --workspace --no-deps
```

### Documentation Standards

- Document all public items
- Include examples in documentation
- Use proper Markdown formatting
- Keep documentation up to date with code changes

## Issue Reporting

### Bug Reports

When reporting bugs, please include:

1. **Description**: Clear description of the issue
2. **Steps to Reproduce**: Numbered list of steps
3. **Expected Behavior**: What you expected to happen
4. **Actual Behavior**: What actually happened
5. **Environment**: OS, Rust version, etc.
6. **Logs**: Any relevant error messages or logs

### Feature Requests

When requesting features, please include:

1. **Description**: Clear description of the feature
2. **Use Case**: Why this feature would be useful
3. **Proposed Solution**: If you have one
4. **Alternatives**: Any alternative solutions considered

## License

By contributing to Maix-Agent, you agree that your contributions will be licensed under the same license as the project (AGPL-3.0-or-later for open source).

## Questions?

If you have questions about contributing, please:

1. Check the [documentation](docs/)
2. Search existing [issues](https://github.com/JularDepick/Maix-Agent/issues)
3. Create a new issue if needed

Thank you for contributing to Maix-Agent!
