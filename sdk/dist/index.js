"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.PROPOSAL_LEN = exports.MULTISIG_LEN = exports.MAX_OWNERS = exports.PROGRAM_ID = void 0;
exports.setProgramId = setProgramId;
exports.findMultisigPda = findMultisigPda;
exports.findVaultPda = findVaultPda;
exports.findProposalPda = findProposalPda;
exports.createMultisigIx = createMultisigIx;
exports.proposeIx = proposeIx;
exports.approveIx = approveIx;
exports.executeSolIx = executeSolIx;
exports.executeTokenIx = executeTokenIx;
exports.executeConfigIx = executeConfigIx;
exports.cancelIx = cancelIx;
exports.depositIx = depositIx;
exports.decodeMultisig = decodeMultisig;
exports.decodeProposal = decodeProposal;
exports.fetchMultisig = fetchMultisig;
exports.fetchProposal = fetchProposal;
exports.fetchProposals = fetchProposals;
exports.sendIx = sendIx;
exports.createMultisig = createMultisig;
exports.propose = propose;
exports.approve = approve;
exports.cancel = cancel;
exports.deposit = deposit;
exports.proposalStatus = proposalStatus;
exports.kindLabel = kindLabel;
const web3_js_1 = require("@solana/web3.js");
const spl_token_1 = require("@solana/spl-token");
/** Default program id. Replace with the deployed address before use. */
exports.PROGRAM_ID = new web3_js_1.PublicKey("11111111111111111111111111111111");
function setProgramId(id) {
    exports.PROGRAM_ID = new web3_js_1.PublicKey(id);
}
exports.MAX_OWNERS = 8;
exports.MULTISIG_LEN = 32 + 32 * exports.MAX_OWNERS + 1 + 1 + 8 + 8 + 8 + 1;
exports.PROPOSAL_LEN = 32 + 8 + 32 + 33 + 32 + 8 + 32 + 8 + 1 + 1 + 1 + 32 * exports.MAX_OWNERS + 1 + 32 * exports.MAX_OWNERS + 1;
// ---------- PDA helpers ----------
function findMultisigPda(creator, nonce) {
    const buf = Buffer.alloc(8);
    buf.writeBigUInt64LE(nonce);
    return web3_js_1.PublicKey.findProgramAddressSync([Buffer.from("multisig"), creator.toBuffer(), buf], exports.PROGRAM_ID);
}
function findVaultPda(multisig) {
    return web3_js_1.PublicKey.findProgramAddressSync([Buffer.from("vault"), multisig.toBuffer()], exports.PROGRAM_ID);
}
function findProposalPda(multisig, index) {
    const buf = Buffer.alloc(8);
    buf.writeBigUInt64LE(index);
    return web3_js_1.PublicKey.findProgramAddressSync([Buffer.from("proposal"), multisig.toBuffer(), buf], exports.PROGRAM_ID);
}
// ---------- borsh arg encoding ----------
function encodeVecPubkey(keys) {
    const out = Buffer.alloc(4 + 32 * keys.length);
    out.writeUInt32LE(keys.length, 0);
    keys.forEach((k, i) => k.toBuffer().copy(out, 4 + 32 * i));
    return out;
}
function encodeProposalKind(kind) {
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
function decodeProposalKind(data, off) {
    const tag = data[off.v++];
    switch (tag) {
        case 0:
            return { tag: "SolTransfer" };
        case 1:
            return { tag: "TokenTransfer" };
        case 2:
            return { tag: "AddOwner", newOwner: new web3_js_1.PublicKey(data.subarray(off.v, (off.v += 32))) };
        case 3:
            return { tag: "RemoveOwner", owner: new web3_js_1.PublicKey(data.subarray(off.v, (off.v += 32))) };
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
function createMultisigIx(creator, owners, threshold, timelockSeconds, nonce) {
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
    const ix = new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: creator, isSigner: true, isWritable: true },
            { pubkey: multisig, isSigner: false, isWritable: true },
            { pubkey: vault, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: Buffer.concat([Buffer.from([0]), args]),
    });
    return { ix, multisig, vault };
}
function proposeIx(proposer, multisig, index, kind, destination, amount, mint) {
    const [proposal] = findProposalPda(multisig, index);
    const amountBuf = Buffer.alloc(8);
    amountBuf.writeBigUInt64LE(amount);
    const args = Buffer.concat([
        encodeProposalKind(kind),
        destination.toBuffer(),
        amountBuf,
        mint.toBuffer(),
    ]);
    const ix = new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: proposer, isSigner: true, isWritable: true },
            { pubkey: multisig, isSigner: false, isWritable: true },
            { pubkey: proposal, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SystemProgram.programId, isSigner: false, isWritable: false },
            { pubkey: web3_js_1.SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
        ],
        data: Buffer.concat([Buffer.from([1]), args]),
    });
    return { ix, proposal };
}
function approveIx(owner, multisig, proposal) {
    return new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: owner, isSigner: true, isWritable: false },
            { pubkey: multisig, isSigner: false, isWritable: false },
            { pubkey: proposal, isSigner: false, isWritable: true },
        ],
        data: Buffer.from([2]),
    });
}
function executeSolIx(executor, multisig, proposal, vault, destination) {
    return new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: executor, isSigner: true, isWritable: false },
            { pubkey: multisig, isSigner: false, isWritable: true },
            { pubkey: proposal, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
            { pubkey: vault, isSigner: false, isWritable: true },
            { pubkey: destination, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: Buffer.from([3]),
    });
}
function executeTokenIx(executor, multisig, proposal, vault, vaultTokenAccount, destinationTokenAccount) {
    return new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: executor, isSigner: true, isWritable: false },
            { pubkey: multisig, isSigner: false, isWritable: true },
            { pubkey: proposal, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
            { pubkey: vault, isSigner: false, isWritable: false },
            { pubkey: vaultTokenAccount, isSigner: false, isWritable: true },
            { pubkey: destinationTokenAccount, isSigner: false, isWritable: true },
            { pubkey: spl_token_1.TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ],
        data: Buffer.from([3]),
    });
}
function executeConfigIx(executor, multisig, proposal) {
    return new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: executor, isSigner: true, isWritable: false },
            { pubkey: multisig, isSigner: false, isWritable: true },
            { pubkey: proposal, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
        ],
        data: Buffer.from([3]),
    });
}
function cancelIx(owner, multisig, proposal) {
    return new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: owner, isSigner: true, isWritable: false },
            { pubkey: multisig, isSigner: false, isWritable: false },
            { pubkey: proposal, isSigner: false, isWritable: true },
        ],
        data: Buffer.from([4]),
    });
}
function depositIx(funder, vault, amount) {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(amount);
    return new web3_js_1.TransactionInstruction({
        programId: exports.PROGRAM_ID,
        keys: [
            { pubkey: funder, isSigner: true, isWritable: true },
            { pubkey: vault, isSigner: false, isWritable: true },
            { pubkey: web3_js_1.SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: Buffer.concat([Buffer.from([5]), b]),
    });
}
// ---------- state decoding ----------
function readPubkey(data, off) {
    const k = new web3_js_1.PublicKey(data.subarray(off.v, off.v + 32));
    off.v += 32;
    return k;
}
function decodeMultisig(address, data) {
    const off = { v: 0 };
    const creator = readPubkey(data, off);
    const owners = [];
    for (let i = 0; i < exports.MAX_OWNERS; i++)
        owners.push(readPubkey(data, off));
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
function decodeProposal(address, data) {
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
    const approvers = [];
    for (let i = 0; i < exports.MAX_OWNERS; i++)
        approvers.push(readPubkey(data, off));
    const cancelCount = data[off.v++];
    const cancelVotes = [];
    for (let i = 0; i < exports.MAX_OWNERS; i++)
        cancelVotes.push(readPubkey(data, off));
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
async function fetchMultisig(connection, address) {
    const info = await connection.getAccountInfo(address);
    if (!info)
        return null;
    return decodeMultisig(address, info.data);
}
async function fetchProposal(connection, address) {
    const info = await connection.getAccountInfo(address);
    if (!info)
        return null;
    return decodeProposal(address, info.data);
}
async function fetchProposals(connection, multisig, count) {
    const out = [];
    for (let i = 0n; i < BigInt(count); i++) {
        const [addr] = findProposalPda(multisig, i);
        const p = await fetchProposal(connection, addr);
        if (p)
            out.push(p);
    }
    return out;
}
// ---------- transaction helpers ----------
async function sendIx(connection, ix, signers, feePayer) {
    const tx = new web3_js_1.Transaction().add(ix);
    tx.feePayer = feePayer;
    const sig = await (0, web3_js_1.sendAndConfirmTransaction)(connection, tx, signers, {
        commitment: "confirmed",
    });
    return sig;
}
async function createMultisig(connection, payer, owners, threshold, timelockSeconds, nonce) {
    const { ix, multisig, vault } = createMultisigIx(payer.publicKey, owners, threshold, timelockSeconds, nonce);
    const signature = await sendIx(connection, ix, [payer], payer.publicKey);
    return { multisig, vault, signature };
}
async function propose(connection, proposer, multisig, kind, destination, amount, mint) {
    const ms = await fetchMultisig(connection, multisig);
    if (!ms)
        throw new Error("multisig not found");
    const { ix, proposal } = proposeIx(proposer.publicKey, multisig, ms.proposalCount, kind, destination, amount, mint);
    const signature = await sendIx(connection, ix, [proposer], proposer.publicKey);
    return { proposal, index: ms.proposalCount, signature };
}
async function approve(connection, owner, multisig, proposal) {
    return sendIx(connection, approveIx(owner.publicKey, multisig, proposal), [owner], owner.publicKey);
}
async function cancel(connection, owner, multisig, proposal) {
    return sendIx(connection, cancelIx(owner.publicKey, multisig, proposal), [owner], owner.publicKey);
}
async function deposit(connection, funder, vault, amount) {
    return sendIx(connection, depositIx(funder.publicKey, vault, amount), [funder], funder.publicKey);
}
function proposalStatus(p, threshold, timelockSeconds, nowSec) {
    if (p.executed)
        return "executed";
    if (p.cancelled)
        return "cancelled";
    if (p.approvals.length < threshold)
        return "draft";
    if (nowSec < p.createdAt + timelockSeconds)
        return "ready";
    return "executable";
}
function kindLabel(kind) {
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
//# sourceMappingURL=index.js.map