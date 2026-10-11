import { Connection, PublicKey, TransactionInstruction, Keypair, Signer } from "@solana/web3.js";
/** Default program id. Replace with the deployed address before use. */
export declare let PROGRAM_ID: PublicKey;
export declare function setProgramId(id: PublicKey | string): void;
export declare const MAX_OWNERS = 8;
export declare const MULTISIG_LEN: number;
export declare const PROPOSAL_LEN: number;
export type ProposalKind = {
    tag: "SolTransfer";
} | {
    tag: "TokenTransfer";
} | {
    tag: "AddOwner";
    newOwner: PublicKey;
} | {
    tag: "RemoveOwner";
    owner: PublicKey;
} | {
    tag: "ChangeThreshold";
    newThreshold: number;
} | {
    tag: "ChangeTimelock";
    newTimelock: bigint;
};
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
export declare function findMultisigPda(creator: PublicKey, nonce: bigint): [PublicKey, number];
export declare function findVaultPda(multisig: PublicKey): [PublicKey, number];
export declare function findProposalPda(multisig: PublicKey, index: bigint): [PublicKey, number];
export declare function createMultisigIx(creator: PublicKey, owners: PublicKey[], threshold: number, timelockSeconds: bigint, nonce: bigint): {
    ix: TransactionInstruction;
    multisig: PublicKey;
    vault: PublicKey;
};
export declare function proposeIx(proposer: PublicKey, multisig: PublicKey, index: bigint, kind: ProposalKind, destination: PublicKey, amount: bigint, mint: PublicKey): {
    ix: TransactionInstruction;
    proposal: PublicKey;
};
export declare function approveIx(owner: PublicKey, multisig: PublicKey, proposal: PublicKey): TransactionInstruction;
export declare function executeSolIx(executor: PublicKey, multisig: PublicKey, proposal: PublicKey, vault: PublicKey, destination: PublicKey): TransactionInstruction;
export declare function executeTokenIx(executor: PublicKey, multisig: PublicKey, proposal: PublicKey, vaultTokenAccount: PublicKey, destinationTokenAccount: PublicKey): TransactionInstruction;
export declare function executeConfigIx(executor: PublicKey, multisig: PublicKey, proposal: PublicKey): TransactionInstruction;
export declare function cancelIx(owner: PublicKey, multisig: PublicKey, proposal: PublicKey): TransactionInstruction;
export declare function depositIx(funder: PublicKey, vault: PublicKey, amount: bigint): TransactionInstruction;
export declare function decodeMultisig(address: PublicKey, data: Buffer): MultisigData;
export declare function decodeProposal(address: PublicKey, data: Buffer): ProposalData;
export declare function fetchMultisig(connection: Connection, address: PublicKey): Promise<MultisigData | null>;
export declare function fetchProposal(connection: Connection, address: PublicKey): Promise<ProposalData | null>;
export declare function fetchProposals(connection: Connection, multisig: PublicKey, count: number): Promise<ProposalData[]>;
export declare function sendIx(connection: Connection, ix: TransactionInstruction, signers: Signer[], feePayer: PublicKey): Promise<string>;
export declare function createMultisig(connection: Connection, payer: Keypair, owners: PublicKey[], threshold: number, timelockSeconds: bigint, nonce: bigint): Promise<{
    multisig: PublicKey;
    vault: PublicKey;
    signature: string;
}>;
export declare function propose(connection: Connection, proposer: Keypair, multisig: PublicKey, kind: ProposalKind, destination: PublicKey, amount: bigint, mint: PublicKey): Promise<{
    proposal: PublicKey;
    index: bigint;
    signature: string;
}>;
export declare function approve(connection: Connection, owner: Keypair, multisig: PublicKey, proposal: PublicKey): Promise<string>;
export declare function cancel(connection: Connection, owner: Keypair, multisig: PublicKey, proposal: PublicKey): Promise<string>;
export declare function deposit(connection: Connection, funder: Keypair, vault: PublicKey, amount: bigint): Promise<string>;
export declare function proposalStatus(p: ProposalData, threshold: number, timelockSeconds: bigint, nowSec: bigint): "draft" | "ready" | "executable" | "executed" | "cancelled";
export declare function kindLabel(kind: ProposalKind): string;
//# sourceMappingURL=index.d.ts.map