# TokenUnion Security Model (Phase 4.5)

## Threat Model

TokenUnion assumes a potentially hostile network and potentially compromised peers.
The primary security goals are:

- API keys remain local to owner machines.
- Compromised peers cannot decrypt or exfiltrate other members' keys.
- Tampered/replayed grant requests are rejected.
- Abuse from a peer is bounded by local rate limits and availability policy.
- Security-relevant actions are auditable.

## Key Protections

## Key Storage

- Provider API keys are encrypted at rest with `age` passphrase mode.
- Passphrase material is derived with PBKDF2 from:
  - user vault password
  - per-device salt (`device_info.device_salt`)
- Without correct password and device salt, key decryption fails.

## Network Trust Boundaries

- P2P transport uses libp2p Noise-encrypted channels.
- Token request control messages are signed with requester ed25519 identity.
- Verifier checks:
  - signature validity
  - signer public key maps to claimed `PeerId`
  - requester ID matches connection peer

## Relay Metadata Limits

- Relay nodes are treated as **metadata-exposed but content-blind**:
  - relay can observe circuit/routing metadata and peer addressing patterns
  - relay cannot decrypt message payloads or API content
- `relay_mode` is explicit and persisted:
  - `off`
  - `self_hosted` (recommended default)
  - `community`
- Runtime automatically disables relay circuits after direct paths are established.
- For LAN-only circles, relay should remain `off`.

## Replay Protection

- Signed token requests include nonce + timestamp.
- Requests outside +/-30 second window are rejected.
- Nonce store (`seen_nonces`) enforces one-time use in window.

## Rate Limiting

- Granting peers enforce per-peer request limits per minute.
- Limits are configurable and tracked in:
  - `peer_rate_limits`
  - `peer_request_window`

## Content Filtering

- Optional local policy (`blocked_model_patterns`) can deny requests by model pattern.

## Auditability

- Every proxied transaction is logged in `audit_log` with:
  - direction
  - peer
  - model
  - token counts
  - request nonce/hash
- Security-sensitive actions/events are logged in `security_events`.

## No Content Persistence

- Prompt/response content is never written to SQLite.
- `log_content` is hard-forced to `false` (not a user-tunable mode).
- Stored telemetry is metadata-only:
  - timestamp
  - peer/model/provider
  - token counts
  - request hash/nonce

## Revocation / Leave Circle

- `Leave circle` broadcasts leave intent to peers and clears local peer tables.
- Local identity rotates to a new ed25519 keypair and persists to DB.
- Circle key is rotated on revocation.
- App restart is required for swarm runtime to bind the new identity.

## Circle Key + Ledger Gossip

- Circle membership is gated by possession of a circle key.
- New members request circle key material via invite flow key exchange.
- Circle key is encrypted at rest locally (`circle_keys`).
- Ledger entries are signed and replicated over gossipsub as encrypted envelopes.
- Offline peers recover via `ledger_sync_request` / `ledger_sync_response` using lamport cursor.
- Merge policy is CRDT-lite:
  - unique `entry_id`
  - lamport ordering
  - last-write-wins only when incoming lamport is newer for same `entry_id`

## Non-Goals (Current Phase)

- Hardware-backed key storage (Secure Enclave/HSM).
- Forward-secure key rotation across historical encrypted blobs.
- Full remote attestation of peer runtime integrity.

## Operational Guidance

- Use strong vault passwords.
- Rotate identity after suspected compromise.
- Monitor `security_events` for replay/signature/rate-limit anomalies.
- Keep `blocked_model_patterns` and rate limits aligned with trust level of peers.
