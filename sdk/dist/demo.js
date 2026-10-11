"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const fs = __importStar(require("fs"));
const web3_js_1 = require("@solana/web3.js");
const index_1 = require("./index");
const PROGRAM_ID = new web3_js_1.PublicKey(process.env.MULTISIG_PROGRAM_ID ?? "");
let failures = 0;
function check(name, cond, detail = "") {
    if (cond) {
        console.log(`  PASS: ${name}`);
    }
    else {
        failures++;
        console.log(`  FAIL: ${name} ${detail}`);
    }
}
async function main() {
    (0, index_1.setProgramId)(PROGRAM_ID);
    const connection = new web3_js_1.Connection("http://127.0.0.1:8899", "confirmed");
    const faucetPath = process.env.FAUCET_KEYPAIR ?? "";
    if (!faucetPath)
        throw new Error("FAUCET_KEYPAIR env var is required");
    const funder = web3_js_1.Keypair.fromSecretKey(Buffer.from(JSON.parse(fs.readFileSync(faucetPath, "utf8"))));
    const owners = [web3_js_1.Keypair.generate(), web3_js_1.Keypair.generate(), web3_js_1.Keypair.generate()];
    const destination = web3_js_1.Keypair.generate().publicKey;
    console.log("Funding owner wallets...");
    for (const kp of owners) {
        const tx = new web3_js_1.Transaction().add(web3_js_1.SystemProgram.transfer({
            fromPubkey: funder.publicKey,
            toPubkey: kp.publicKey,
            lamports: 10 * web3_js_1.LAMPORTS_PER_SOL,
        }));
        await (0, web3_js_1.sendAndConfirmTransaction)(connection, tx, [funder]);
    }
    console.log("Creating 2-of-3 multisig (timelock 8s)...");
    const { multisig, vault } = await (0, index_1.createMultisig)(connection, owners[0], owners.map((o) => o.publicKey), 2, 8n, 99n);
    console.log(`  multisig: ${multisig.toBase58()}`);
    console.log(`  vault:    ${vault.toBase58()}`);
    let ms = await (0, index_1.fetchMultisig)(connection, multisig);
    check("multisig has 3 owners", ms !== null && ms.owners.length === 3);
    check("threshold is 2", ms !== null && ms.threshold === 2);
    console.log("Depositing 5 SOL into the vault...");
    await (0, index_1.deposit)(connection, owners[0], vault, BigInt(5 * web3_js_1.LAMPORTS_PER_SOL));
    const vaultBal = await connection.getBalance(vault);
    check("vault holds ~5 SOL", vaultBal >= 5 * web3_js_1.LAMPORTS_PER_SOL - 1000);
    console.log("Owner 0 proposes 1 SOL transfer...");
    const amount = BigInt(web3_js_1.LAMPORTS_PER_SOL);
    const { proposal } = await (0, index_1.propose)(connection, owners[0], multisig, { tag: "SolTransfer" }, destination, amount, web3_js_1.PublicKey.default);
    let p = await (0, index_1.fetchProposal)(connection, proposal);
    check("proposer counts as first approval", p !== null && p.approvals.length === 1);
    console.log("Owner 1 approves (threshold reached)...");
    await (0, index_1.approve)(connection, owners[1], multisig, proposal);
    p = await (0, index_1.fetchProposal)(connection, proposal);
    check("two approvals recorded", p !== null && p.approvals.length === 2);
    console.log("Trying to execute before timelock expires (must fail)...");
    const earlyIx = (0, index_1.executeSolIx)(owners[0].publicKey, multisig, proposal, vault, destination);
    let earlyFailed = false;
    try {
        await (0, index_1.sendIx)(connection, earlyIx, [owners[0]], owners[0].publicKey);
    }
    catch {
        earlyFailed = true;
    }
    check("early execution rejected", earlyFailed);
    console.log("Waiting out the 8s timelock...");
    await new Promise((r) => setTimeout(r, 10000));
    console.log("Executing...");
    const execIx = (0, index_1.executeSolIx)(owners[2].publicKey, multisig, proposal, vault, destination);
    await (0, index_1.sendIx)(connection, execIx, [owners[2]], owners[2].publicKey);
    const destBal = await connection.getBalance(destination);
    check("destination received 1 SOL", destBal === web3_js_1.LAMPORTS_PER_SOL, `got ${destBal}`);
    p = await (0, index_1.fetchProposal)(connection, proposal);
    check("proposal marked executed", p !== null && p.executed);
    console.log("Owner 0 proposes, then cancels...");
    const { proposal: prop2 } = await (0, index_1.propose)(connection, owners[0], multisig, { tag: "SolTransfer" }, destination, amount, web3_js_1.PublicKey.default);
    await (0, index_1.cancel)(connection, owners[0], multisig, prop2);
    const p2 = await (0, index_1.fetchProposal)(connection, prop2);
    check("proposal cancelled", p2 !== null && p2.cancelled);
    console.log("Owner management: add a fourth owner via proposal...");
    const newcomer = web3_js_1.Keypair.generate().publicKey;
    const { proposal: prop3 } = await (0, index_1.propose)(connection, owners[0], multisig, { tag: "AddOwner", newOwner: newcomer }, web3_js_1.PublicKey.default, 0n, web3_js_1.PublicKey.default);
    await (0, index_1.approve)(connection, owners[1], multisig, prop3);
    await new Promise((r) => setTimeout(r, 10000));
    const [, v2] = (0, index_1.findVaultPda)(multisig);
    void v2;
    const execCfg = (0, index_1.executeConfigIx)(owners[0].publicKey, multisig, prop3);
    await (0, index_1.sendIx)(connection, execCfg, [owners[0]], owners[0].publicKey);
    ms = await (0, index_1.fetchMultisig)(connection, multisig);
    check("fourth owner added", ms !== null && ms.owners.length === 4);
    const nowSec = BigInt(Math.floor(Date.now() / 1000));
    const st = p2 && ms ? (0, index_1.proposalStatus)(p2, ms.threshold, ms.timelockSeconds, nowSec) : "draft";
    check("cancelled proposal reports cancelled status", st === "cancelled");
    console.log(failures === 0 ? "ALL CHECKS PASSED" : `${failures} CHECKS FAILED`);
    process.exit(failures === 0 ? 0 : 1);
}
main().catch((e) => {
    console.error(e);
    process.exit(1);
});
//# sourceMappingURL=demo.js.map