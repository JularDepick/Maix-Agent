# Changelog

All notable changes to Maix-Agent will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- GitHub Actions CI/CD pipeline
- Infrastructure documentation (CHANGELOG, SECURITY, CONTRIBUTING)

## [0.1.2] - 2026-05-17

### Added
- LSP (Language Server Protocol) support
- Cron job scheduling system
- Git worktree management
- WebFetch AI capabilities
- Multimodal support (images, files)
- Skill installation system
- Windows Service support
- Named Pipe transport
- Auto-update mechanism
- Cost tracking and budget management
- Session fork functionality
- Enhanced hooks system

### Fixed
- CLI ask showing reasoning process (now hidden by default, --verbose to show)
- Version number inconsistency across crates (unified to 0.1.2)
- TUI Chinese character truncation panic
- TUI empty input cursor out-of-bounds panic

## [0.1.1] - 2026-05-13

### Added
- TUI optimizations and polish
- Agent features and improvements
- Documentation updates

### Changed
- Improved input handling
- Enhanced status bar display
- Better error messages

## [0.1.0] - 2026-05-12

### Added
- Initial release
- Core daemon engine (maix.exe) with gRPC server
- CLI client (maix-cli.exe)
- TUI client (maix-tui.exe) with vim mode
- HTTP gateway (maix-gateway.exe)
- Multi-agent parallel/async collaboration
- Human-like long-term memory system
- Multi-model routing (DeepSeek, MiniMax, Anthropic, OpenAI)
- MCP protocol client/server
- Hooks system
- Skills system
- TOML-based architecture DSL
- SQLite database with WAL mode
- 83+ built-in tools (fs, shell, network, git, LSP, MCP, cron, etc.)
- Hierarchical, collaborative, and debate topologies
- Episodic, semantic, and working memory types

## [0.0.1] - 2026-05-01

### Added
- Project initialization
- Basic architecture design
- Core type definitions

---

## Release Notes

### Version 0.1.2
This release focuses on advanced features and platform integration. Key additions include LSP support for code intelligence, cron job scheduling for automated tasks, and Windows Service support for production deployment. The auto-update mechanism ensures users always have the latest features.

### Version 0.1.1
Performance and usability improvements to the TUI client, including better input handling, enhanced status bar, and improved error messages. Agent features have been expanded with better multi-agent coordination.

### Version 0.1.0
The initial public release of Maix-Agent, providing a complete AI agent framework with multi-model support, long-term memory, and extensive tooling. This release establishes the foundation for building sophisticated AI applications.
