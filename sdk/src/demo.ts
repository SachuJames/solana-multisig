import * as fs from "fs";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  approve,
  cancel,
  createMultisig,
  deposit,
  executeConfigIx,
  executeSolIx,
  fetchMultisig,
  fetchProposal,
  findVaultPda,
  propose,
  proposalStatus,
  sendIx,
  setProgramId,
} from "./index";

const PROGRAM_ID = new PublicKey(process.env.MULTISIG_PROGRAM_ID ?? "");

let failures = 0;
function check(name: string, cond: boolean, detail = "") {
  if (cond) {
    console.log(`  PASS: ${name}`);
  } else {
    failures++;
    console.log(`  FAIL: ${name} ${detail}`);
  }
}

async function main() {
  setProgramId(PROGRAM_ID);
  const connection = new Connection("http://127.0.0.1:8899", "confirmed");
  const faucetPath = process.env.FAUCET_KEYPAIR ?? "";
  if (!faucetPath) throw new Error("FAUCET_KEYPAIR env var is required");
  const funder = Keypair.fromSecretKey(
    Buffer.from(JSON.parse(fs.readFileSync(faucetPath, "utf8")))
  );

  const owners = [Keypair.generate(), Keypair.generate(), Keypair.generate()];
  const destination = Keypair.generate().publicKey;

  console.log("Funding owner wallets...");
  for (const kp of owners) {
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: funder.publicKey,
        toPubkey: kp.publicKey,
        lamports: 2 * LAMPORTS_PER_SOL,
      })
    );
    await sendAndConfirmTransaction(connection, tx, [funder]);
  }

  console.log("Creating 2-of-3 multisig (timelock 8s)...");
  const { multisig, vault } = await createMultisig(
    connection,
    owners[0],
    owners.map((o) => o.publicKey),
    2,
    8n,
    99n
  );
  console.log(`  multisig: ${multisig.toBase58()}`);
  console.log(`  vault:    ${vault.toBase58()}`);

  let ms = await fetchMultisig(connection, multisig);
  check("multisig has 3 owners", ms !== null && ms.owners.length === 3);
  check("threshold is 2", ms !== null && ms.threshold === 2);

  console.log("Depositing 5 SOL into the vault...");
  await deposit(connection, owners[0], vault, BigInt(5 * LAMPORTS_PER_SOL));
  const vaultBal = await connection.getBalance(vault);
  check("vault holds ~5 SOL", vaultBal >= 5 * LAMPORTS_PER_SOL - 1000);

  console.log("Owner 0 proposes 1 SOL transfer...");
  const amount = BigInt(LAMPORTS_PER_SOL);
  const { proposal } = await propose(
    connection,
    owners[0],
    multisig,
    { tag: "SolTransfer" },
    destination,
    amount,
    PublicKey.default
  );
  let p = await fetchProposal(connection, proposal);
  check("proposer counts as first approval", p !== null && p.approvals.length === 1);

  console.log("Owner 1 approves (threshold reached)...");
  await approve(connection, owners[1], multisig, proposal);
  p = await fetchProposal(connection, proposal);
  check("two approvals recorded", p !== null && p.approvals.length === 2);

  console.log("Trying to execute before timelock expires (must fail)...");
  const earlyIx = executeSolIx(owners[0].publicKey, multisig, proposal, vault, destination);
  let earlyFailed = false;
  try {
    await sendIx(connection, earlyIx, [owners[0]], owners[0].publicKey);
  } catch {
    earlyFailed = true;
  }
  check("early execution rejected", earlyFailed);

  console.log("Waiting out the 8s timelock...");
  await new Promise((r) => setTimeout(r, 10000));

  console.log("Executing...");
  const execIx = executeSolIx(owners[2].publicKey, multisig, proposal, vault, destination);
  await sendIx(connection, execIx, [owners[2]], owners[2].publicKey);
  const destBal = await connection.getBalance(destination);
  check("destination received 1 SOL", destBal === LAMPORTS_PER_SOL, `got ${destBal}`);
  p = await fetchProposal(connection, proposal);
  check("proposal marked executed", p !== null && p.executed);

  console.log("Owner 0 proposes, then cancels...");
  const { proposal: prop2 } = await propose(
    connection,
    owners[0],
    multisig,
    { tag: "SolTransfer" },
    destination,
    amount,
    PublicKey.default
  );
  await cancel(connection, owners[0], multisig, prop2);
  const p2 = await fetchProposal(connection, prop2);
  check("proposal cancelled", p2 !== null && p2.cancelled);

  console.log("Owner management: add a fourth owner via proposal...");
  const newcomer = Keypair.generate().publicKey;
  const { proposal: prop3 } = await propose(
    connection,
    owners[0],
    multisig,
    { tag: "AddOwner", newOwner: newcomer },
    PublicKey.default,
    0n,
    PublicKey.default
  );
  await approve(connection, owners[1], multisig, prop3);
  await new Promise((r) => setTimeout(r, 10000));
  const [, v2] = findVaultPda(multisig);
  void v2;
  const execCfg = executeConfigIx(owners[0].publicKey, multisig, prop3);
  await sendIx(connection, execCfg, [owners[0]], owners[0].publicKey);
  ms = await fetchMultisig(connection, multisig);
  check("fourth owner added", ms !== null && ms.owners.length === 4);

  const nowSec = BigInt(Math.floor(Date.now() / 1000));
  const st = p2 && ms ? proposalStatus(p2, ms.threshold, ms.timelockSeconds, nowSec) : "draft";
  check("cancelled proposal reports cancelled status", st === "cancelled");

  console.log(failures === 0 ? "ALL CHECKS PASSED" : `${failures} CHECKS FAILED`);
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
