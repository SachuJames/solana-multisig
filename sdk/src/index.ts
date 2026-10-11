import {
  Connection,
  PublicKey,
  SYSVAR_CLOCK_PUBKEY,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
  Keypair,
  Signer,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

/** Default program id. Replace with the deployed address before use. */
export let PROGRAM_ID = new PublicKey("11111111111111111111111111111111");

export function setProgramId(id: PublicKey | string) {
  PROGRAM_ID = new PublicKey(id);
}

export const MAX_OWNERS = 8;
export const MULTISIG_LEN = 32 + 32 * MAX_OWNERS + 1 + 1 + 8 + 8 + 8 + 1;
export const PROPOSAL_LEN =
  32 + 8 + 32 + 33 + 32 + 8 + 32 + 8 + 1 + 1 + 1 + 32 * MAX_OWNERS + 1 + 32 * MAX_OWNERS + 1;

export type ProposalKind =
  | { tag: "SolTransfer" }
  | { tag: "TokenTransfer" }
  | { tag: "AddOwner"; newOwner: PublicKey }
  | { tag: "RemoveOwner"; owner: PublicKey }
  | { tag: "ChangeThreshold"; newThreshold: number }
  | { tag: "ChangeTimelock"; newTimelock: bigint };

export interface MultisigData {
  address: PublicKey;
  creator: PublicKey;
  owners: PublicKey[];
  threshold: number;
  timelockSeconds: bigint;
  proposalCount: bigint;
  nonce: bigint;
  bump: number;
}

export interface ProposalData {
  address: PublicKey;
  multisig: PublicKey;
  index: bigint;
  proposer: PublicKey;
  kind: ProposalKind;
  destination: PublicKey;
  amount: bigint;
  mint: PublicKey;
  createdAt: bigint;
  executed: boolean;
  cancelled: boolean;
  approvals: PublicKey[];
  cancelVotes: PublicKey[];
  bump: number;
}

// ---------- PDA helpers ----------

export function findMultisigPda(creator: PublicKey, nonce: bigint): [PublicKey, number] {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(nonce);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("multisig"), creator.toBuffer(), buf],
    PROGRAM_ID
  );
}

export function findVaultPda(multisig: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), multisig.toBuffer()],
    PROGRAM_ID
  );
}

export function findProposalPda(multisig: PublicKey, index: bigint): [PublicKey, number] {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(index);
  return PublicKey.findProgramAddressSync(
    [Buffer.from("proposal"), multisig.toBuffer(), buf],
    PROGRAM_ID
  );
}

// ---------- borsh arg encoding ----------

function encodeVecPubkey(keys: PublicKey[]): Buffer {
  const out = Buffer.alloc(4 + 32 * keys.length);
  out.writeUInt32LE(keys.length, 0);
  keys.forEach((k, i) => k.toBuffer().copy(out, 4 + 32 * i));
  return out;
}

function encodeProposalKind(kind: ProposalKind): Buffer {
  switch (kind.tag) {
    case "SolTransfer":
      return Buffer.from([0]);
    case "TokenTransfer":
      return Buffer.from([1]);
    case "AddOwner":
      return Buffer.concat([Buffer.from([2]), kind.newOwner.toBuffer()]);
    case "RemoveOwner":
      return Buffer.concat([Buffer.from([3]), kind.owner.toBuffer()]);
    case "ChangeThreshold":
      return Buffer.from([4, kind.newThreshold]);
    case "ChangeTimelock": {
      const b = Buffer.alloc(9);
      b[0] = 5;
      b.writeBigUInt64LE(kind.newTimelock, 1);
      return b;
    }
  }
}

function decodeProposalKind(data: Buffer, off: { v: number }): ProposalKind {
  const tag = data[off.v++];
  switch (tag) {
    case 0:
      return { tag: "SolTransfer" };
    case 1:
      return { tag: "TokenTransfer" };
    case 2:
      return { tag: "AddOwner", newOwner: new PublicKey(data.subarray(off.v, (off.v += 32))) };
    case 3:
      return { tag: "RemoveOwner", owner: new PublicKey(data.subarray(off.v, (off.v += 32))) };
    case 4:
      return { tag: "ChangeThreshold", newThreshold: data[off.v++] };
    case 5: {
      const v = data.readBigUInt64LE(off.v);
      off.v += 8;
      return { tag: "ChangeTimelock", newTimelock: v };
    }
    default:
      throw new Error(`unknown proposal kind tag ${tag}`);
  }
}

// ---------- instruction builders ----------

export function createMultisigIx(
  creator: PublicKey,
  owners: PublicKey[],
  threshold: number,
  timelockSeconds: bigint,
  nonce: bigint
): { ix: TransactionInstruction; multisig: PublicKey; vault: PublicKey } {
  const [multisig] = findMultisigPda(creator, nonce);
  const [vault] = findVaultPda(multisig);
  const args = Buffer.concat([
    encodeVecPubkey(owners),
    Buffer.from([threshold]),
    (() => {
      const b = Buffer.alloc(8);
      b.writeBigUInt64LE(timelockSeconds);
      return b;
    })(),
    (() => {
      const b = Buffer.alloc(8);
      b.writeBigUInt64LE(nonce);
      return b;
    })(),
  ]);
  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: creator, isSigner: true, isWritable: true },
      { pubkey: multisig, isSigner: false, isWritable: true },
      { pubkey: vault, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([0]), args]),
  });
  return { ix, multisig, vault };
}

export function proposeIx(
  proposer: PublicKey,
  multisig: PublicKey,
  index: bigint,
  kind: ProposalKind,
  destination: PublicKey,
  amount: bigint,
  mint: PublicKey
): { ix: TransactionInstruction; proposal: PublicKey } {
  const [proposal] = findProposalPda(multisig, index);
  const amountBuf = Buffer.alloc(8);
  amountBuf.writeBigUInt64LE(amount);
  const args = Buffer.concat([
    encodeProposalKind(kind),
    destination.toBuffer(),
    amountBuf,
    mint.toBuffer(),
  ]);
  const ix = new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: proposer, isSigner: true, isWritable: false },
      { pubkey: multisig, isSigner: false, isWritable: false },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([1]), args]),
  });
  return { ix, proposal };
}

export function approveIx(
  owner: PublicKey,
  multisig: PublicKey,
  proposal: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: owner, isSigner: true, isWritable: false },
      { pubkey: multisig, isSigner: false, isWritable: false },
      { pubkey: proposal, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([2]),
  });
}

export function executeSolIx(
  executor: PublicKey,
  multisig: PublicKey,
  proposal: PublicKey,
  vault: PublicKey,
  destination: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: executor, isSigner: true, isWritable: false },
      { pubkey: multisig, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: vault, isSigner: false, isWritable: true },
      { pubkey: destination, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([3]),
  });
}

export function executeTokenIx(
  executor: PublicKey,
  multisig: PublicKey,
  proposal: PublicKey,
  vaultTokenAccount: PublicKey,
  destinationTokenAccount: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: executor, isSigner: true, isWritable: false },
      { pubkey: multisig, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: vaultTokenAccount, isSigner: false, isWritable: true },
      { pubkey: destinationTokenAccount, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([3]),
  });
}

export function executeConfigIx(
  executor: PublicKey,
  multisig: PublicKey,
  proposal: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: executor, isSigner: true, isWritable: false },
      { pubkey: multisig, isSigner: false, isWritable: true },
      { pubkey: proposal, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([3]),
  });
}

export function cancelIx(
  owner: PublicKey,
  multisig: PublicKey,
  proposal: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: owner, isSigner: true, isWritable: false },
      { pubkey: multisig, isSigner: false, isWritable: false },
      { pubkey: proposal, isSigner: false, isWritable: true },
    ],
    data: Buffer.from([4]),
  });
}

export function depositIx(
  funder: PublicKey,
  vault: PublicKey,
  amount: bigint
): TransactionInstruction {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(amount);
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: funder, isSigner: true, isWritable: true },
      { pubkey: vault, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([Buffer.from([5]), b]),
  });
}

// ---------- state decoding ----------

function readPubkey(data: Buffer, off: { v: number }): PublicKey {
  const k = new PublicKey(data.subarray(off.v, off.v + 32));
  off.v += 32;
  return k;
}

export function decodeMultisig(address: PublicKey, data: Buffer): MultisigData {
  const off = { v: 0 };
  const creator = readPubkey(data, off);
  const owners: PublicKey[] = [];
  for (let i = 0; i < MAX_OWNERS; i++) owners.push(readPubkey(data, off));
  const ownerCount = data[off.v++];
  const threshold = data[off.v++];
  const timelockSeconds = data.readBigUInt64LE(off.v);
  off.v += 8;
  const proposalCount = data.readBigUInt64LE(off.v);
  off.v += 8;
  const nonce = data.readBigUInt64LE(off.v);
  off.v += 8;
  const bump = data[off.v];
  return {
    address,
    creator,
    owners: owners.slice(0, ownerCount),
    threshold,
    timelockSeconds,
    proposalCount,
    nonce,
    bump,
  };
}

export function decodeProposal(address: PublicKey, data: Buffer): ProposalData {
  const off = { v: 0 };
  const multisig = readPubkey(data, off);
  const index = data.readBigUInt64LE(off.v);
  off.v += 8;
  const proposer = readPubkey(data, off);
  const kind = decodeProposalKind(data, off);
  const destination = readPubkey(data, off);
  const amount = data.readBigUInt64LE(off.v);
  off.v += 8;
  const mint = readPubkey(data, off);
  const createdAt = data.readBigInt64LE(off.v);
  off.v += 8;
  const executed = data[off.v++] === 1;
  const cancelled = data[off.v++] === 1;
  const approvalCount = data[off.v++];
  const approvers: PublicKey[] = [];
  for (let i = 0; i < MAX_OWNERS; i++) approvers.push(readPubkey(data, off));
  const cancelCount = data[off.v++];
  const cancelVotes: PublicKey[] = [];
  for (let i = 0; i < MAX_OWNERS; i++) cancelVotes.push(readPubkey(data, off));
  const bump = data[off.v];
  return {
    address,
    multisig,
    index,
    proposer,
    kind,
    destination,
    amount,
    mint,
    createdAt,
    executed,
    cancelled,
    approvals: approvers.slice(0, approvalCount),
    cancelVotes: cancelVotes.slice(0, cancelCount),
    bump,
  };
}

export async function fetchMultisig(
  connection: Connection,
  address: PublicKey
): Promise<MultisigData | null> {
  const info = await connection.getAccountInfo(address);
  if (!info) return null;
  return decodeMultisig(address, info.data);
}

export async function fetchProposal(
  connection: Connection,
  address: PublicKey
): Promise<ProposalData | null> {
  const info = await connection.getAccountInfo(address);
  if (!info) return null;
  return decodeProposal(address, info.data);
}

export async function fetchProposals(
  connection: Connection,
  multisig: PublicKey,
  count: number
): Promise<ProposalData[]> {
  const out: ProposalData[] = [];
  for (let i = 0n; i < BigInt(count); i++) {
    const [addr] = findProposalPda(multisig, i);
    const p = await fetchProposal(connection, addr);
    if (p) out.push(p);
  }
  return out;
}

// ---------- transaction helpers ----------

export async function sendIx(
  connection: Connection,
  ix: TransactionInstruction,
  signers: Signer[],
  feePayer: PublicKey
): Promise<string> {
  const tx = new Transaction().add(ix);
  tx.feePayer = feePayer;
  const sig = await sendAndConfirmTransaction(connection, tx, signers, {
    commitment: "confirmed",
  });
  return sig;
}

export async function createMultisig(
  connection: Connection,
  payer: Keypair,
  owners: PublicKey[],
  threshold: number,
  timelockSeconds: bigint,
  nonce: bigint
): Promise<{ multisig: PublicKey; vault: PublicKey; signature: string }> {
  const { ix, multisig, vault } = createMultisigIx(
    payer.publicKey,
    owners,
    threshold,
    timelockSeconds,
    nonce
  );
  const signature = await sendIx(connection, ix, [payer], payer.publicKey);
  return { multisig, vault, signature };
}

export async function propose(
  connection: Connection,
  proposer: Keypair,
  multisig: PublicKey,
  kind: ProposalKind,
  destination: PublicKey,
  amount: bigint,
  mint: PublicKey
): Promise<{ proposal: PublicKey; index: bigint; signature: string }> {
  const ms = await fetchMultisig(connection, multisig);
  if (!ms) throw new Error("multisig not found");
  const { ix, proposal } = proposeIx(
    proposer.publicKey,
    multisig,
    ms.proposalCount,
    kind,
    destination,
    amount,
    mint
  );
  const signature = await sendIx(connection, ix, [proposer], proposer.publicKey);
  return { proposal, index: ms.proposalCount, signature };
}

export async function approve(
  connection: Connection,
  owner: Keypair,
  multisig: PublicKey,
  proposal: PublicKey
): Promise<string> {
  return sendIx(
    connection,
    approveIx(owner.publicKey, multisig, proposal),
    [owner],
    owner.publicKey
  );
}

export async function cancel(
  connection: Connection,
  owner: Keypair,
  multisig: PublicKey,
  proposal: PublicKey
): Promise<string> {
  return sendIx(
    connection,
    cancelIx(owner.publicKey, multisig, proposal),
    [owner],
    owner.publicKey
  );
}

export async function deposit(
  connection: Connection,
  funder: Keypair,
  vault: PublicKey,
  amount: bigint
): Promise<string> {
  return sendIx(
    connection,
    depositIx(funder.publicKey, vault, amount),
    [funder],
    funder.publicKey
  );
}

export function proposalStatus(
  p: ProposalData,
  threshold: number,
  timelockSeconds: bigint,
  nowSec: bigint
): "draft" | "ready" | "executable" | "executed" | "cancelled" {
  if (p.executed) return "executed";
  if (p.cancelled) return "cancelled";
  if (p.approvals.length < threshold) return "draft";
  if (nowSec < p.createdAt + timelockSeconds) return "ready";
  return "executable";
}

export function kindLabel(kind: ProposalKind): string {
  switch (kind.tag) {
    case "SolTransfer":
      return "SOL transfer";
    case "TokenTransfer":
      return "Token transfer";
    case "AddOwner":
      return `Add owner ${kind.newOwner.toBase58().slice(0, 8)}…`;
    case "RemoveOwner":
      return `Remove owner ${kind.owner.toBase58().slice(0, 8)}…`;
    case "ChangeThreshold":
      return `Threshold → ${kind.newThreshold}`;
    case "ChangeTimelock":
      return `Timelock → ${kind.newTimelock}s`;
  }
}
