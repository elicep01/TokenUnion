# TokenUnion

A peer-to-peer desktop application for cooperative API credit pooling across trusted circles. Built with Tauri 2, React, and libp2p.

TokenUnion runs as a local proxy that intercepts API calls, coordinates with peers via encrypted P2P messaging, and routes requests through available members — enabling small groups to share idle API credits across time zones without exposing keys.

## Problem

API subscriptions bill monthly but usage is bursty. Credits go unused while you sleep, and reset before peers in other time zones can benefit. There is no mechanism to share surplus capacity within a trusted group without centralizing keys on a third-party service.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         TokenUnion Desktop                         │
│                                                                     │
│  ┌──────────────────────┐       ┌────────────────────────────────┐  │
│  │     React Frontend   │       │        Tauri / Rust Backend    │  │
│  │                      │       │                                │  │
│  │  ┌────────────────┐  │       │  ┌──────────┐  ┌───────────┐  │  │
│  │  │  Dashboard     │  │ IPC   │  │  Vault   │  │  Tracker  │  │  │
│  │  │  Circle        │◄─┼───────┼─►│ (age +   │  │ (token    │  │  │
│  │  │  Ledger        │  │ Tauri │  │  PBKDF2) │  │  counting)│  │  │
│  │  │  Vault         │  │ cmds  │  └──────────┘  └───────────┘  │  │
│  │  │  Schedule      │  │       │                                │  │
│  │  │  Settings      │  │       │  ┌──────────┐  ┌───────────┐  │  │
│  │  │  Onboarding    │  │       │  │  SQLite  │  │   Proxy   │  │  │
│  │  └────────────────┘  │       │  │  (db.rs) │  │ :47821    │  │  │
│  │                      │       │  └──────────┘  └─────┬─────┘  │  │
│  │  Zustand state mgmt  │       │                      │        │  │
│  │  Recharts viz        │       │  ┌───────────────────┴──────┐ │  │
│  └──────────────────────┘       │  │      libp2p Swarm       │ │  │
│                                 │  │  TCP + Noise + Yamux     │ │  │
│                                 │  │  mDNS · Relay · Dcutr   │ │  │
│                                 │  │  Gossipsub · Req/Res    │ │  │
│                                 │  └────────────┬────────────┘ │  │
│                                 └───────────────┼──────────────┘  │
└─────────────────────────────────────────────────┼─────────────────┘
                                                  │
                      ┌───────────────────────────┼──────────────────┐
                      │                           │                  │
               ┌──────▼──────┐            ┌───────▼───────┐   ┌─────▼─────┐
               │  Peer Node  │            │  Peer Node    │   │   Relay   │
               │  (same LAN  │            │  (remote,     │   │  (Go,     │
               │   via mDNS) │            │   via relay)  │   │  libp2p)  │
               └─────────────┘            └───────────────┘   └───────────┘
```

### Request Routing Flow

```
  Developer Tool (Claude Code, Codex, etc.)
        │
        │  HTTP request to localhost:47821
        ▼
  ┌─────────────┐
  │  Local Proxy │
  │  (axum)      │
  └──────┬──────┘
         │
         ▼
  ┌──────────────────┐     YES     ┌──────────────────┐
  │ Local key         ├────────────►│ Forward to        │
  │ available?        │             │ upstream API      │
  └───────┬──────────┘             └────────┬─────────┘
          │ NO                              │
          ▼                                 ▼
  ┌──────────────────┐             ┌──────────────────┐
  │ Broadcast grant   │             │ Track tokens,     │
  │ request to peers  │             │ log to ledger     │
  └───────┬──────────┘             └──────────────────┘
          │
          ▼
  ┌──────────────────┐
  │ Peer executes     │
  │ request with      │
  │ their own key     │
  └───────┬──────────┘
          │
          ▼
  ┌──────────────────┐
  │ Response relayed  │
  │ back via P2P      │
  └──────────────────┘
```

## Security Model

| Property | Implementation |
|---|---|
| Key storage | Encrypted at rest via `age` + PBKDF2-derived passphrase |
| Key isolation | API keys never leave the local machine; peers proxy requests, not credentials |
| Message authentication | All P2P messages signed with ed25519 keypairs |
| Replay protection | Nonce + ±30s timestamp window per request |
| Rate limiting | Per-peer configurable request limits with sliding window |
| Content privacy | Prompt/response payloads are never persisted to local telemetry or audit tables |
| Ledger integrity | Transactions include SHA-256 request hashes; replicated via gossipsub |
| Relay trust model | Relays are metadata-aware but content-blind (Noise encryption) |

See [SECURITY.md](SECURITY.md) for the full threat model and mitigation details.

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | React 18, TypeScript, Tailwind CSS, Zustand, Recharts |
| Desktop shell | Tauri 2 (Rust) |
| HTTP proxy | Axum + Reqwest |
| P2P networking | rust-libp2p (TCP, Noise, Yamux, mDNS, Relay, Dcutr, Gossipsub, Request-Response) |
| Storage | SQLite via rusqlite (bundled) |
| Encryption | age, AES-GCM, PBKDF2, SHA-256 |
| Relay server | Go + go-libp2p |
| CI/CD | GitHub Actions |

## Project Structure

```
├── src/                    # React frontend
│   ├── pages/              #   Dashboard, Circle, Ledger, Vault, Schedule, Settings, Onboarding
│   ├── components/         #   Layout, Nav, TrayPopover
│   ├── stores/             #   Zustand app store
│   └── styles/             #   Tailwind globals
├── src-tauri/              # Rust backend
│   └── src/
│       ├── main.rs         #   Tauri command bridge
│       ├── db.rs           #   SQLite schema + queries
│       ├── p2p.rs          #   libp2p swarm, protocols, messaging
│       ├── proxy.rs        #   Local HTTP proxy (axum)
│       ├── tracker.rs      #   Token usage accounting
│       └── vault.rs        #   Key encryption/decryption
├── relay/                  # Go relay server
│   └── main.go
└── .github/workflows/      # CI/CD pipeline
```

## Getting Started

### Prerequisites

- Node.js 20+
- Rust toolchain via [rustup](https://rustup.rs) (`cargo` must be in `$PATH`)

### Development

```bash
npm install
npm run tauri dev
```

### Proxy Configuration

Point your AI tools at the local proxy:

```bash
# Anthropic (Claude Code, etc.)
export ANTHROPIC_BASE_URL=http://localhost:47821

# OpenAI-compatible (Codex, etc.)
export OPENAI_BASE_URL=http://localhost:47821
export OPENAI_API_KEY=dummy
```

## Usage

1. Launch TokenUnion and complete onboarding (identity, keys, circle, schedule).
2. Add API keys in **Vault** or authenticate via Anthropic OAuth (PKCE).
3. Create or join a **Circle** using an invite link.
4. Configure your **Schedule** — a 7×24 weekly availability grid controlling when your credits are shared.
5. Set your tool's base URL to the local proxy.
6. Use your tools as normal. TokenUnion handles routing, peer coordination, and ledger tracking transparently.

## Current Status

Implemented: local proxy routing, P2P circle networking, credit pooling with grant protocol, ledger tracking with gossipsub replication, weekly schedule controls, Phase 4.5 security hardening, and a compact dark-themed UI.

In progress: OAuth automation refinements and additional operational tooling.
