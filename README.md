# TokenUnion

TokenUnion is a desktop app for small friend circles that use AI coding tools. It helps the group share API credits in a practical way. If one person is sleeping and not using their credits, another person in a different time zone can borrow through the circle.

The app is local first. Your keys stay on your own machine. The app runs as a local proxy and coordinates with trusted peers in your circle.

## What problem this solves

Many people pay for API usage but do not use all of it every day. Credits reset and unused value is lost. Also, friends working in different time zones are rarely active at the same time.

TokenUnion solves this by routing requests through available peers only when needed. It tracks lending and borrowing so the group can stay fair.

## Why this design was chosen

TokenUnion is built as a desktop app because key security matters. The app does not need a central cloud service to hold secrets.

The design uses peer to peer networking so your friend circle is the trust boundary. This means the system still works when internet services are unreliable, and private data does not need to sit on a third party backend.

## Architecture in simple words

TokenUnion has four main parts.

1. Frontend app

This is the interface you see. It is made with React. It handles onboarding, dashboard, vault, schedule, ledger, and settings.

2. Local backend inside the desktop app

This is written in Rust and runs in Tauri. It stores local data in SQLite, manages encryption, runs background jobs, and exposes commands to the frontend.

3. Local proxy

Your tools call `http://localhost:47821`. TokenUnion receives the request, decides whether to use your own key or a peer key, forwards the request, and returns the response.

4. Peer to peer network

Circle members connect through libp2p. On the same network, peers can discover each other automatically. Across networks, a relay can help connect peers. Message content is encrypted in transit.

## Security model in plain English

TokenUnion is designed around these rules.

1. API keys are encrypted at rest.

2. API keys are never sent to other peers.

3. Token requests are signed and checked.

4. Replay attempts are rejected using nonce and time window checks.

5. Per peer rate limits are enforced.

6. Prompt and response content is not persisted to local telemetry tables.

7. Circle key material is encrypted locally and used for encrypted ledger gossip.

8. Relay nodes are treated as metadata exposed but content blind.

For deeper details, read `SECURITY.md`.

## Product flow

1. Install and open TokenUnion.

2. Complete onboarding.

3. Add your API keys in Vault or connect Anthropic OAuth.

4. Create or join a circle.

5. Set your sharing schedule.

6. Point your tools to the local proxy.

7. Work normally while TokenUnion routes and tracks usage.

## How routing works

When a request comes in, TokenUnion checks if your local key is available.

If available, it uses your local key.

If not available, it asks online peers for a grant.

The first valid grant is used.

The chosen peer makes the upstream API call on their own machine.

The response comes back to you.

Both sides store metadata for ledger and audit purposes.

## Tech stack

Frontend uses React, TypeScript, Tailwind, and Zustand.

Desktop backend uses Tauri 2 and Rust.

Proxy uses axum and reqwest.

Peer networking uses rust libp2p with TCP, Noise, Yamux, mDNS, relay, dcutr, request response, and gossipsub.

Storage uses SQLite through rusqlite.

Encryption uses age plus PBKDF2 based derivation for protected local data.

## Local development setup

Install Node.js 20 or newer.

Install Rust using rustup and make sure `cargo` is available.

Run:

```bash
npm install
npm run tauri dev
```

If you get an error that says `cargo metadata` is missing, Rust is not installed correctly or not in your shell path.

## Proxy quick start

For Anthropic tools:

```bash
export ANTHROPIC_BASE_URL=http://localhost:47821
```

For OpenAI or Codex tools:

```bash
export OPENAI_BASE_URL=http://localhost:47821
export OPENAI_API_KEY=dummy
```

## Current status

This project includes local proxying, circle networking, credit pooling, ledger tracking, schedule controls, security hardening, and a redesigned compact UI.

Some advanced pieces are still improving over time, such as smoother OAuth automation and additional operational tooling.
