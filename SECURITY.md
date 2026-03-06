# TokenUnion Security Model

## Threat Model

TokenUnion assumes a potentially hostile network and potentially compromised peers.

Primary security goals:

- API keys remain local to the owner's machine.
- Compromised peers cannot decrypt or exfiltrate other members' keys.
- Tampered or replayed grant requests are rejected.
- Abuse from any peer is bounded by local rate limits and availability policy.
- Security-relevant actions are auditable.

## Key Storage

- Provider API keys are encrypted at rest using `age` passphrase mode.
- Passphrase material is derived via PBKDF2 from:
  - User vault password
  - Per-device salt (`device_info.device_salt`)
- Decryption fails without the correct password and device salt.

## Network Trust Boundaries

- P2P transport uses libp2p Noise-encrypted channels.
- Token request control messages are signed with the requester's ed25519 identity key.
- Verification checks:
  - Signature validity
  - Signer public key maps to the claimed `PeerId`
  - Requester ID matches the connection peer

## Relay Metadata Exposure

Relay nodes are treated as **metadata-exposed but content-blind**:

- Relays can observe circuit/routing metadata and peer addressing patterns.
- Relays cannot decrypt message payloads or API content.
- `relay_mode` is explicit and persisted:
  - `off` — no relay (LAN-only circles should use this)
  - `self_hosted` — recommended default
  - `community`
- The runtime automatically disables relay circuits after direct paths are established.

## Replay Protection

- Signed token requests include a nonce and timestamp.
- Requests outside a ±30 second window are rejected.
- The nonce store (`seen_nonces`) enforces one-time use within the window.

## Rate Limiting

- Granting peers enforce per-peer request limits per minute.
- Limits are configurable and tracked in `peer_rate_limits` and `peer_request_window`.

## Content Filtering

- An optional local policy (`blocked_model_patterns`) can deny requests by model name pattern.

## Auditability

Every proxied transaction is logged in `audit_log` with:

- Direction (lent / borrowed / self)
- Peer identity
- Model and provider
- Token counts
- Request nonce and SHA-256 hash

Security-sensitive events (replay attempts, signature failures, rate limit violations) are logged separately in `security_events`.

## Content Privacy

- Prompt and response content is never written to SQLite.
- `log_content` is hard-set to `false` — this is not a user-configurable option.
- Stored telemetry is metadata-only: timestamp, peer, model, provider, token counts, request hash, and nonce.

## Circle Key and Ledger Gossip

- Circle membership is gated by possession of a shared circle key.
- New members receive circle key material via the invite flow key exchange.
- The circle key is encrypted at rest locally (`circle_keys`).
- Ledger entries are signed and replicated over gossipsub as encrypted envelopes.
- Offline peers recover via `ledger_sync_request` / `ledger_sync_response` using a Lamport cursor.
- Merge policy is CRDT-lite:
  - Unique `entry_id` per transaction
  - Lamport ordering
  - Last-write-wins only when the incoming Lamport clock is newer for the same `entry_id`

## Revocation and Circle Departure

- `Leave circle` broadcasts leave intent to peers and clears local peer tables.
- The local identity rotates to a new ed25519 keypair, persisted to the database.
- The circle key is rotated on revocation.
- An app restart is required for the swarm runtime to bind the new identity.

## Non-Goals

- Hardware-backed key storage (Secure Enclave / HSM)
- Forward-secure key rotation across historical encrypted blobs
- Full remote attestation of peer runtime integrity

## Operational Guidance

- Use strong vault passwords.
- Rotate identity after suspected compromise.
- Monitor `security_events` for replay, signature, and rate-limit anomalies.
- Keep `blocked_model_patterns` and rate limits aligned with the trust level of your peers.
