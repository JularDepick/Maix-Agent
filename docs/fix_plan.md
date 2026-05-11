# Maix-Agent v0.1.0 Beta — Fix Plan

Test date: 2026-05-10–11 | Binary versions: v0.1.0 | Scope: all 4 binaries

---

## Issue Summary

| # | Severity | Component | Title | Status |
|---|----------|-----------|-------|--------|
| 1 | Critical | maix-server | Provider selection non-deterministic + double `/v1` → 404 | Fixed |
| 2 | High | maix-gateway | `--help` / `--version` panics (no CLI parsing) | Fixed |
| 3 | High | maix-gateway | Port conflict with gRPC server (both bind 26506) | Fixed |
| 4 | High | maix-gateway | gRPC server address hardcoded to 127.0.0.1:26506 | Fixed |
| 5 | High | maix-core | `config show` exposes API key in plaintext | Fixed |
| 6 | Medium | maix-cli | Chat REPL infinite loop on stdin EOF | Fixed |
| 7 | Medium | maix-cli | Duplicate text output (TextDelta + Complete) | Fixed |
| 8 | Medium | maix-cli | Reasoning text runs into response text | Fixed |
| 9 | Medium | all | "Connected to maix-server" stale naming | Fixed |
| 10 | Medium | maix-cli, maix-server | Version strings wrong (`maix-cli`→"maix", `maix`→"maix-server") | Fixed |
| 11 | Medium | maix-cli | `-m` / `--model` flag not accessible from subcommands | Fixed |
| 12 | Medium | maix-server | `forget_memory` returns true for non-existent IDs | Fixed |
| 13 | Low | all | Verbose "Connected to maix-server v..." on every command | Fixed |
| 14 | Low | maix-server | `transport_mode` variable unused (dead code) | Fixed |

---

## Remaining Minor Issues (v0.2.0)

| # | Severity | Component | Title |
|---|----------|-----------|-------|
| 15 | Low | maix-gateway | Empty TextDelta produces `{"content":"","type":"text"}` SSE events |
| 16 | Low | maix-gateway | Session `message_count` stays 0 after chat |
| 17 | Low | maix-server | Work status `total_tokens` not updated from chat sessions | Fixed |
| 18 | Low | maix-gateway | `importance` float precision (`0.800000011920929` vs `0.8`) | Fixed |
| 19 | Low | maix-cli | `--model` flag accepted but ignored (server picks provider, not client) | |
| 20 | Low | maix-server | Unused functions in `client_launcher.rs`, `transport.rs` | Fixed |
| 21 | Low | maix-cli | Reasoning text per-chunk dim/reset causes visual flicker on some terminals | |
| 22 | Low | maix-gateway | `session_id` field in `ChatRequest` accepted but unused | Fixed |
| 23 | Low | maix-cli | `--server` flag not accessible from subcommands (missing global = true) | Fixed |
| 24 | Low | all | Build warnings (unused fields, functions, variants) | Fixed |
| 25 | Medium | maix-memory | Empty query memory search returns no results | Fixed |
| 26 | Low | maix-gateway | Health endpoint missing `active_sessions` and `queue_depth` | Fixed |
| 27 | High | maix-agent | Multi-turn chat fails with reasoning_content error (tool-call path) | Fixed |

---

## Verification Checklist

- [x] `maix ask "hello"` returns a real AI response (not 404)
- [x] `maix-gateway --help` prints help and exits 0
- [x] `maix-gateway --version` prints version and exits 0
- [x] `maix-gateway --listen 0.0.0.0:8080 --server 127.0.0.1:26506` starts without port conflict
- [x] `curl http://localhost:8080/health` returns JSON `{"status":"ok",...}`
- [x] `curl -X POST http://localhost:8080/v1/chat -H 'Content-Type: application/json' -d '{"message":"hello"}'` streams SSE
- [x] `echo "/exit" | maix-cli chat` exits cleanly
- [x] `printf '' | maix-cli chat` exits cleanly (no infinite loop)
- [x] `maix-cli config show` masks the API key (shows `sk-1c3e...fea`)
- [x] `maix-cli memory forget nonexistent` shows "Not found: nonexistent"
- [x] `maix --version` prints `maix 0.1.0` (not maix-server)
- [x] `maix-cli --version` prints `maix-cli 0.1.0` (not maix)
- [x] `maix-cli ask -m deepseek-chat "hello"` works (global flag)
- [x] All 66 tests pass: `cargo test --workspace`
- [x] Full build: `cargo build --workspace`
- [x] Zero compiler warnings: `cargo build --workspace`
- [x] `total_tokens` updates after chat sessions (was always 0)
- [x] `session_id` in gateway ChatRequest enables session reuse
- [x] Float importance displays as `0.8` not `0.800000011920929`
- [x] `maix-cli ask --server 127.0.0.1:26506 "hello"` works (global flag)

---

## Architecture Compliance

- Binary names: `maix.exe` (core engine), `maix-cli.exe`, `maix-tui.exe`, `maix-gateway.exe`
- Client dependency chain: `maix-cli/maix-tui/maix-gateway` → `maix-core (proto + client)` + `tonic` → gRPC → `maix`
- `maix-client` crate removed; `MaixClient` lives in `maix_core::client`
