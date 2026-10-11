<div align="center">
  <img src="docs/hero.webp" width="100%" alt="Multisig treasury hero banner" />
</div>

<div align="center">

[![License: MIT](https://img.shields.io/badge/License-MIT-14f195?style=flat-square)](LICENSE)
[![Solana](https://img.shields.io/badge/Solana-native_program-9945ff?style=flat-square&logo=solana)](https://solana.com)
[![Rust](https://img.shields.io/badge/Rust-borsh-orange?style=flat-square&logo=rust)](program/)
[![TypeScript](https://img.shields.io/badge/TypeScript-SDK_%2B_dashboard-3178c6?style=flat-square&logo=typescript)](sdk/)

**On-chain multisig treasury for Solana.** m-of-n owner approvals, timelocked
execution, and a PDA vault for SOL and SPL tokens. No Anchor, just a native
Rust program with borsh state.

</div>

## How it works

1. **Create** a multisig: pick up to 8 owners, a threshold (e.g. 2-of-3),
   and a timelock in seconds. A PDA vault is created alongside it.
2. **Fund** the vault with SOL (or create token accounts owned by the vault PDA).
3. **Propose**: any owner proposes a SOL transfer, an SPL token transfer, or
   a config change (add/remove owner, change threshold, change timelock).
   The proposer counts as the first approver.
4. **Approve**: owners approve once each. At threshold, the proposal becomes
   eligible but stays locked until `created_at + timelock` passes.
5. **Execute**: anyone can execute once the timelock expires. Funds move out
   of the PDA vault; config changes apply to the multisig account.
6. **Cancel**: the proposer can cancel anytime before execution; other owners
   can force-cancel with threshold many cancel votes.

Config changes (including owner management) go through the same
propose/approve/timelock/execute pipeline, so no single key can ever
unilaterally change the rules.

## Features

- **Native program, no Anchor**: hand-rolled instruction dispatch and
  account validation, borsh-serialized state
- **PDA vault**: SOL and SPL tokens live in program-owned vaults, moved only
  by program-signed transfers
- **Timelock**: every execution waits out a configurable delay after reaching
  threshold
- **Checked math everywhere**: overflow fails the instruction, never wraps
- **SPL token support**: vault token accounts validated by mint and owner
  before every transfer
- **TypeScript SDK**: PDA derivation, instruction builders, state decoding,
  and a full lifecycle demo script
- **React dashboard**: treasury overview, proposal list with live statuses,
  approve/execute/cancel actions, mock mode included

## Quickstart

Prerequisites: Rust toolchain, Node 18+, and the Solana CLI
(`solana`, `solana-test-validator`, `cargo-build-sbf`).

```bash
# Full end-to-end run: starts a local validator, builds and deploys the
# program, then runs a complete multisig lifecycle through the SDK:
# create 2-of-3 -> deposit -> propose -> approve to threshold ->
# early execute (rejected) -> wait out timelock -> execute ->
# cancel flow -> owner management via proposal.
./demo.sh
```

The SDK and dashboard:

```bash
cd sdk && npm install && npm run build
cd dashboard && npm install && npm run build   # then npm run dev
```

Note: `demo.sh` prefers a 2.1.x validator when one is installed next to the
workspace (the 4.x validator needs the `io_uring` syscall, which some
sandboxes block). If your sandbox also blocks UDP `sendto()`, the script
builds and preloads `scripts/udp_sendto_shim.c`, a tiny shim that keeps the
validator's QUIC traffic flowing. On a normal machine neither is needed.

## Testing

```bash
cd program
cargo test            # 3 unit + 9 integration tests (in-process banks client)
```

The integration suite covers: multisig creation and bad configs, propose and
approve to threshold (including double-approve and non-owner rejection),
timelock blocking early execution, SOL execution after timelock with balance
checks, proposer cancel, owner-vote cancel, owner management via proposal
(add, threshold change, removal guard), and SPL token transfers.

## Project layout

```
program/          native Rust program (error, state, instruction, processor)
program/tests/    integration tests (solana-program-test banks client)
sdk/              TypeScript SDK + lifecycle demo script
sdk/src/demo.ts   full propose/approve/timelock/execute/cancel demo
dashboard/        React UI (treasury view, proposals, actions, mock mode)
scripts/          UDP shim for sandboxed validators
demo.sh           end-to-end: validator -> build -> deploy -> SDK demo
```

## Account design

- `multisig` PDA `[b"multisig", creator, nonce]`: owners, threshold,
  timelock, proposal counter
- `vault` PDA `[b"vault", multisig]`: holds SOL; signs token transfers
- `proposal` PDA `[b"proposal", multisig, index]`: kind, destination, amount,
  approvals, cancel votes, timestamps, executed/cancelled flags

## License

MIT. See [LICENSE](LICENSE).
