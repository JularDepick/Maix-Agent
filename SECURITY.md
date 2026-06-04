# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues.**

Instead, please report them via email to: [INSERT SECURITY EMAIL]

You should receive a response within 48 hours. If for some reason you do not, please follow up to ensure we received your original message.

Please include the following information:

- Type of issue (e.g. buffer overflow, SQL injection, cross-site scripting, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

This information will help us triage your report more quickly.

## Preferred Languages

We prefer all communications to be in English or Chinese.

## Security Best Practices

### For Users

1. **Keep Updated**: Always use the latest version of Maix-Agent
2. **Secure Configuration**: Don't expose API keys in configuration files
3. **Network Security**: Use TLS for production deployments
4. **Access Control**: Limit who can access the daemon and gateway

### For Developers

1. **Input Validation**: Always validate and sanitize user inputs
2. **Dependency Security**: Regularly run `cargo audit` to check for vulnerabilities
3. **Secrets Management**: Never commit secrets, API keys, or credentials
4. **Code Review**: All security-related changes require review
5. **Testing**: Include security tests for new features

## Security Features

### Current

- [ ] Input validation and sanitization
- [ ] Environment variable-based secrets management
- [ ] SQLite WAL mode for concurrent access safety
- [ ] gRPC transport security

### Planned

- [ ] TLS encryption for all transports
- [ ] Client authentication mechanisms
- [ ] Credential file support (credentials.json)
- [ ] Process isolation for external skills
- [ ] Rate limiting per client
- [ ] Message size limits enforcement

## Security Contacts

- Primary: [INSERT SECURITY EMAIL]
- Secondary: [INSERT SECONDARY CONTACT]

## Disclosure Policy

When we receive a security bug report, we will:

1. Confirm the problem and determine affected versions
2. Audit code to find similar problems
3. Prepare fixes for all supported versions
4. Release new versions with fixes
5. Publicly disclose the vulnerability after patches are released

## Credits

We appreciate the security research community's efforts in responsibly disclosing vulnerabilities. Researchers who report valid security issues will be acknowledged in our security advisories (unless they prefer to remain anonymous).
