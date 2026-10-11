import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  approveIx,
  cancelIx,
  createMultisigIx,
  decodeMultisig,
  decodeProposal,
  depositIx,
  executeConfigIx,
  executeSolIx,
  fetchProposals,
  findMultisigPda,
  findProposalPda,
  findVaultPda,
  kindLabel,
  proposeIx,
  proposalStatus,
  setProgramId,
  MultisigData,
  ProposalData,
} from "@multisig-treasury/sdk";

const RPC_URL = "http://127.0.0.1:8899";

function short(k: PublicKey): string {
  const s = k.toBase58();
  return s.slice(0, 4) + "…" + s.slice(-4);
}

function sol(lamports: bigint): string {
  return (Number(lamports) / LAMPORTS_PER_SOL).toFixed(4);
}

export default function App() {
  const [programIdInput, setProgramIdInput] = useState("");
  const [secretInput, setSecretInput] = useState("");
  const [keypair, setKeypair] = useState<Keypair | null>(null);
  const [msInput, setMsInput] = useState("");
  const [multisig, setMultisig] = useState<MultisigData | null>(null);
  const [proposals, setProposals] = useState<ProposalData[]>([]);
  const [vaultBalance, setVaultBalance] = useState<bigint>(0n);
  const [now, setNow] = useState(() => BigInt(Math.floor(Date.now() / 1000)));
  const [status, setStatus] = useState("");
  const [mock, setMock] = useState(false);

  // create-multisig form
  const [newOwners, setNewOwners] = useState("");
  const [newThreshold, setNewThreshold] = useState("2");
  const [newTimelock, setNewTimelock] = useState("3600");

  // propose form
  const [dest, setDest] = useState("");
  const [amount, setAmount] = useState("0.1");

  const connection = useMemo(() => new Connection(RPC_URL, "confirmed"), []);

  useEffect(() => {
    const t = setInterval(() => setNow(BigInt(Math.floor(Date.now() / 1000))), 3000);
    return () => clearInterval(t);
  }, []);

  const connectWallet = useCallback(() => {
    try {
      const bytes = Uint8Array.from(
        secretInput
          .trim()
          .split(/[\s,]+/)
          .map((x) => parseInt(x, 10))
      );
      setKeypair(Keypair.fromSecretKey(bytes));
      setStatus("wallet loaded: " + Keypair.fromSecretKey(bytes).publicKey.toBase58());
    } catch {
      setStatus("invalid secret key (comma or space separated bytes)");
    }
  }, [secretInput]);

  const applyProgramId = useCallback(() => {
    try {
      setProgramId(new PublicKey(programIdInput.trim()));
      setStatus("program id set");
    } catch {
      setStatus("invalid program id");
    }
  }, [programIdInput]);

  const refresh = useCallback(async () => {
    if (!programIdInput || !msInput || mock) return;
    try {
      const pid = new PublicKey(programIdInput.trim());
      setProgramId(pid);
      const msAddr = new PublicKey(msInput.trim());
      const info = await connection.getAccountInfo(msAddr);
      if (!info) {
        setStatus("multisig not found");
        return;
      }
      const ms = decodeMultisig(msAddr, info.data);
      setMultisig(ms);
      const [vault] = findVaultPda(msAddr);
      const bal = await connection.getBalance(vault);
      setVaultBalance(BigInt(bal));
      const list = await fetchProposals(connection, msAddr, Number(ms.proposalCount));
      setProposals(list.reverse());
      setStatus(`loaded ${list.length} proposals`);
    } catch (e) {
      setStatus(`refresh failed: ${String(e)}`);
    }
  }, [connection, programIdInput, msInput, mock]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const runTx = useCallback(
    async (label: string, makeIx: () => any) => {
      if (!keypair) {
        setStatus("load a wallet first");
        return;
      }
      try {
        setStatus(label + "…");
        const ix = makeIx();
        const tx = new Transaction().add(ix);
        const sig = await sendAndConfirmTransaction(connection, tx, [keypair], {
          commitment: "confirmed",
        });
        setStatus(`${label} ok: ${sig.slice(0, 16)}…`);
        await refresh();
      } catch (e) {
        setStatus(`${label} failed: ${String(e).slice(0, 160)}`);
      }
    },
    [connection, keypair, refresh]
  );

  const doCreateMultisig = () => {
    if (!keypair) return setStatus("load a wallet first");
    const owners = newOwners
      .split(/[\s,]+/)
      .filter(Boolean)
      .map((s) => new PublicKey(s.trim()));
    const nonce = BigInt(Date.now() % 100000);
    runTx("create multisig", () => {
      const { ix, multisig: addr } = createMultisigIx(
        keypair.publicKey,
        owners,
        parseInt(newThreshold, 10),
        BigInt(newTimelock),
        nonce
      );
      setMsInput(addr.toBase58());
      return ix;
    });
  };

  const doPropose = () => {
    if (!keypair || !multisig) return;
    const lamports = BigInt(Math.floor(parseFloat(amount) * LAMPORTS_PER_SOL));
    runTx("propose", () => {
      const { ix } = proposeIx(
        keypair.publicKey,
        multisig.address,
        multisig.proposalCount,
        { tag: "SolTransfer" },
        new PublicKey(dest.trim()),
        lamports,
        PublicKey.default
      );
      return ix;
    });
  };

  const doDeposit = () => {
    if (!keypair || !multisig) return;
    runTx("deposit 1 SOL", () => {
      const [vault] = findVaultPda(multisig.address);
      return depositIx(keypair.publicKey, vault, BigInt(LAMPORTS_PER_SOL));
    });
  };

  const renderProposal = (p: ProposalData) => {
    if (!multisig || !keypair) return null;
    const st = proposalStatus(p, multisig.threshold, multisig.timelockSeconds, now);
    const executableAt = p.createdAt + multisig.timelockSeconds;
    const isOwner = multisig.owners.some((o) => o.equals(keypair.publicKey));
    const alreadyApproved = p.approvals.some((a) => a.equals(keypair.publicKey));
    return (
      <div key={p.address.toBase58()} className="card proposal">
        <div className="prop-head">
          <span className="prop-index">#{p.index.toString()}</span>
          <span className={`badge ${st}`}>{st}</span>
          <span className="prop-kind">{kindLabel(p.kind)}</span>
        </div>
        <div className="prop-meta">
          <span>to {short(p.destination)}</span>
          <span>{sol(p.amount)} SOL</span>
          <span>
            approvals {p.approvals.length}/{multisig.threshold}
          </span>
          <span>
            {st === "ready"
              ? `unlocks in ${Number(executableAt - now)}s`
              : st === "draft"
              ? "needs approvals"
              : ""}
          </span>
        </div>
        <div className="prop-approvers">
          {p.approvals.map((a) => (
            <span key={a.toBase58()} className="chip ok">
              {short(a)}
            </span>
          ))}
        </div>
        <div className="row">
          {isOwner && !alreadyApproved && (st === "draft" || st === "ready") && (
            <button
              onClick={() =>
                runTx("approve", () => approveIx(keypair.publicKey, multisig.address, p.address))
              }
            >
              Approve
            </button>
          )}
          {st === "executable" && (
            <button
              className="primary"
              onClick={() =>
                runTx("execute", () => {
                  const [vault] = findVaultPda(multisig.address);
                  return p.kind.tag === "SolTransfer"
                    ? executeSolIx(keypair.publicKey, multisig.address, p.address, vault, p.destination)
                    : executeConfigIx(keypair.publicKey, multisig.address, p.address);
                })
              }
            >
              Execute
            </button>
          )}
          {isOwner && (st === "draft" || st === "ready") && (
            <button
              className="danger"
              onClick={() =>
                runTx("cancel", () => cancelIx(keypair.publicKey, multisig.address, p.address))
              }
            >
              Cancel
            </button>
          )}
        </div>
      </div>
    );
  };

  const mockProposals: ProposalData[] = mock
    ? [
        {
          address: PublicKey.default,
          multisig: PublicKey.default,
          index: 2n,
          proposer: keypair?.publicKey ?? PublicKey.default,
          kind: { tag: "SolTransfer" },
          destination: PublicKey.default,
          amount: BigInt(0.5 * LAMPORTS_PER_SOL),
          mint: PublicKey.default,
          createdAt: now - 4000n,
          executed: false,
          cancelled: false,
          approvals: [keypair?.publicKey ?? PublicKey.default],
          cancelVotes: [],
          bump: 0,
        },
        {
          address: PublicKey.default,
          multisig: PublicKey.default,
          index: 1n,
          proposer: PublicKey.default,
          kind: { tag: "ChangeThreshold", newThreshold: 3 },
          destination: PublicKey.default,
          amount: 0n,
          mint: PublicKey.default,
          createdAt: now - 9000n,
          executed: true,
          cancelled: false,
          approvals: [],
          cancelVotes: [],
          bump: 0,
        },
      ]
    : [];

  return (
    <div className="app">
      <header>
        <h1>
          Multisig <span>Treasury</span>
        </h1>
        <p>m-of-n approvals · timelocked execution · PDA vault</p>
      </header>

      <div className="card">
        <h2>Connection</h2>
        <div className="row">
          <input
            placeholder="program id"
            value={programIdInput}
            onChange={(e) => setProgramIdInput(e.target.value)}
            className="grow"
          />
          <button onClick={applyProgramId}>Set</button>
          <label className="mock-toggle">
            <input type="checkbox" checked={mock} onChange={(e) => setMock(e.target.checked)} />
            mock mode
          </label>
        </div>
        <div className="row">
          <input
            placeholder="wallet secret key (bytes, comma separated)"
            value={secretInput}
            onChange={(e) => setSecretInput(e.target.value)}
            className="grow"
            type="password"
          />
          <button onClick={connectWallet}>Load wallet</button>
        </div>
        <div className="row">
          <input
            placeholder="multisig address"
            value={msInput}
            onChange={(e) => setMsInput(e.target.value)}
            className="grow"
          />
          <button onClick={refresh}>Load</button>
        </div>
        {status && <div className="status">{status}</div>}
      </div>

      {!mock && (
        <div className="card">
          <h2>Create multisig</h2>
          <input
            placeholder="owner pubkeys, comma separated"
            value={newOwners}
            onChange={(e) => setNewOwners(e.target.value)}
            className="grow"
          />
          <div className="row">
            <input
              placeholder="threshold"
              value={newThreshold}
              onChange={(e) => setNewThreshold(e.target.value)}
            />
            <input
              placeholder="timelock seconds"
              value={newTimelock}
              onChange={(e) => setNewTimelock(e.target.value)}
            />
            <button className="primary" onClick={doCreateMultisig}>
              Create
            </button>
          </div>
        </div>
      )}

      {(multisig || mock) && (
        <div className="card">
          <h2>Treasury</h2>
          {multisig ? (
            <>
              <div className="grid2">
                <div>
                  <label>threshold</label>
                  <strong>
                    {multisig.threshold} of {multisig.owners.length}
                  </strong>
                </div>
                <div>
                  <label>timelock</label>
                  <strong>{multisig.timelockSeconds.toString()}s</strong>
                </div>
                <div>
                  <label>vault balance</label>
                  <strong>{sol(vaultBalance)} SOL</strong>
                </div>
                <div>
                  <label>proposals</label>
                  <strong>{multisig.proposalCount.toString()}</strong>
                </div>
              </div>
              <div className="owners">
                {multisig.owners.map((o) => (
                  <span key={o.toBase58()} className="chip">
                    {short(o)}
                  </span>
                ))}
              </div>
              <div className="row">
                <button onClick={doDeposit}>Deposit 1 SOL</button>
              </div>
            </>
          ) : (
            <div className="grid2">
              <div>
                <label>threshold</label>
                <strong>2 of 3 (mock)</strong>
              </div>
              <div>
                <label>vault balance</label>
                <strong>42.5000 SOL (mock)</strong>
              </div>
            </div>
          )}
        </div>
      )}

      {(multisig || mock) && (
        <div className="card">
          <h2>New proposal (SOL transfer)</h2>
          <div className="row">
            <input
              placeholder="destination"
              value={dest}
              onChange={(e) => setDest(e.target.value)}
              className="grow"
            />
            <input
              placeholder="amount SOL"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
            />
            <button className="primary" onClick={doPropose} disabled={mock}>
              Propose
            </button>
          </div>
        </div>
      )}

      <h2 className="section-title">Proposals</h2>
      {(mock ? mockProposals : proposals).map((p) =>
        mock
          ? (() => {
              const st = proposalStatus(p, 2, 3600n, now);
              return (
                <div key={p.index.toString()} className="card proposal">
                  <div className="prop-head">
                    <span className="prop-index">#{p.index.toString()}</span>
                    <span className={`badge ${st}`}>{st} (mock)</span>
                    <span className="prop-kind">{kindLabel(p.kind)}</span>
                  </div>
                  <div className="prop-meta">
                    <span>{sol(p.amount)} SOL</span>
                    <span>approvals {p.approvals.length}/2</span>
                  </div>
                </div>
              );
            })()
          : renderProposal(p)
      )}
      {!mock && proposals.length === 0 && multisig && (
        <p className="empty">no proposals yet</p>
      )}

      <footer>
        <span>multisig treasury · local validator demo</span>
      </footer>
    </div>
  );
}
