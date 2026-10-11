use borsh::{BorshDeserialize, BorshSerialize};
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_program_test::{processor, BanksClientError, ProgramTest, ProgramTestContext};
use solana_pubkey::{pubkey, Pubkey};
use solana_signer::Signer;
use solana_transaction::Transaction;

use multisig_treasury::instruction::{CreateMultisigArgs, ProposeArgs, TreasuryInstruction};
use multisig_treasury::state::{Multisig, Proposal, ProposalKind, MAX_OWNERS};

const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");
const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const CLOCK_SYSVAR_ID: Pubkey = pubkey!("SysvarC1ock11111111111111111111111111111111");

fn program_id() -> Pubkey {
    Pubkey::new_unique()
}

fn multisig_pda(creator: &Pubkey, nonce: u64, pid: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"multisig", creator.as_ref(), &nonce.to_le_bytes()],
        pid,
    )
}

fn vault_pda(multisig: &Pubkey, pid: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault", multisig.as_ref()], pid)
}

fn proposal_pda(multisig: &Pubkey, index: u64, pid: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"proposal", multisig.as_ref(), &index.to_le_bytes()],
        pid,
    )
}

async fn start_context() -> (ProgramTestContext, Pubkey) {
    let pid = program_id();
    let mut pt = ProgramTest::new(
        "multisig_treasury",
        pid,
        processor!(multisig_treasury::processor::process_instruction),
    );
    pt.add_program(
        "spl_token",
        spl_token::id(),
        processor!(spl_token::processor::Processor::process),
    );
    (pt.start_with_context().await, pid)
}

async fn send_tx(
    ctx: &mut ProgramTestContext,
    ixs: &[Instruction],
    signers: &[&Keypair],
) -> Result<(), BanksClientError> {
    let recent = ctx.banks_client.get_latest_blockhash().await.unwrap();
    let msg = Message::new_with_blockhash(ixs, Some(&ctx.payer.pubkey()), &recent);
    let mut tx = Transaction::new_unsigned(msg);
    let mut all: Vec<&dyn Signer> = vec![&ctx.payer];
    for s in signers {
        all.push(*s);
    }
    tx.sign(&all, recent);
    ctx.banks_client.process_transaction(tx).await
}

async fn fund(ctx: &mut ProgramTestContext, to: &Pubkey, lamports: u64) {
    let ix = Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(ctx.payer.pubkey(), true),
            AccountMeta::new(*to, false),
        ],
        data: {
            let mut d = Vec::with_capacity(12);
            d.extend_from_slice(&2u32.to_le_bytes());
            d.extend_from_slice(&lamports.to_le_bytes());
            d
        },
    };
    send_tx(ctx, &[ix], &[]).await.unwrap();
}

async fn clock_now(ctx: &mut ProgramTestContext) -> i64 {
    let clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp
}

/// Warp forward until the test clock reaches `target` (safety-capped).
async fn warp_to(ctx: &mut ProgramTestContext, target: i64) {
    for _ in 0..200 {
        let c: Clock = ctx.banks_client.get_sysvar().await.unwrap();
        if c.unix_timestamp >= target {
            return;
        }
        ctx.warp_to_slot(c.slot + 5000).unwrap();
    }
    panic!("warp_to: clock did not advance to {}", target);
}

fn ix_create_multisig(
    pid: &Pubkey,
    creator: &Pubkey,
    owners: Vec<Pubkey>,
    threshold: u8,
    timelock: u64,
    nonce: u64,
) -> (Instruction, Pubkey, Pubkey) {
    let (ms, _) = multisig_pda(creator, nonce, pid);
    let (vault, _) = vault_pda(&ms, pid);
    let args = CreateMultisigArgs {
        owners,
        threshold,
        timelock_seconds: timelock,
        nonce,
    };
    let ix = Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*creator, true),
            AccountMeta::new(ms, false),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: TreasuryInstruction::CreateMultisig(args).pack(),
    };
    (ix, ms, vault)
}

fn ix_propose(
    pid: &Pubkey,
    proposer: &Pubkey,
    multisig: &Pubkey,
    index: u64,
    kind: ProposalKind,
    destination: Pubkey,
    amount: u64,
    mint: Pubkey,
) -> (Instruction, Pubkey) {
    let (prop, _) = proposal_pda(multisig, index, pid);
    let args = ProposeArgs {
        kind,
        destination,
        amount,
        mint,
    };
    let ix = Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*proposer, true),
            AccountMeta::new(*multisig, false),
            AccountMeta::new(prop, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(CLOCK_SYSVAR_ID, false),
        ],
        data: TreasuryInstruction::Propose(args).pack(),
    };
    (ix, prop)
}

fn ix_approve(pid: &Pubkey, owner: &Pubkey, multisig: &Pubkey, proposal: &Pubkey) -> Instruction {
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new_readonly(*owner, true),
            AccountMeta::new_readonly(*multisig, false),
            AccountMeta::new(*proposal, false),
        ],
        data: TreasuryInstruction::Approve.pack(),
    }
}

fn ix_execute_sol(
    pid: &Pubkey,
    executor: &Pubkey,
    multisig: &Pubkey,
    proposal: &Pubkey,
    vault: &Pubkey,
    destination: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new_readonly(*executor, true),
            AccountMeta::new(*multisig, false),
            AccountMeta::new(*proposal, false),
            AccountMeta::new_readonly(CLOCK_SYSVAR_ID, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new(*destination, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: TreasuryInstruction::Execute.pack(),
    }
}

fn ix_execute_config(
    pid: &Pubkey,
    executor: &Pubkey,
    multisig: &Pubkey,
    proposal: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new_readonly(*executor, true),
            AccountMeta::new(*multisig, false),
            AccountMeta::new(*proposal, false),
            AccountMeta::new_readonly(CLOCK_SYSVAR_ID, false),
        ],
        data: TreasuryInstruction::Execute.pack(),
    }
}

fn ix_cancel(pid: &Pubkey, owner: &Pubkey, multisig: &Pubkey, proposal: &Pubkey) -> Instruction {
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new_readonly(*owner, true),
            AccountMeta::new_readonly(*multisig, false),
            AccountMeta::new(*proposal, false),
        ],
        data: TreasuryInstruction::Cancel.pack(),
    }
}

fn ix_deposit(pid: &Pubkey, funder: &Pubkey, vault: &Pubkey, amount: u64) -> Instruction {
    Instruction {
        program_id: *pid,
        accounts: vec![
            AccountMeta::new(*funder, true),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: TreasuryInstruction::Deposit { amount }.pack(),
    }
}

async fn fetch_multisig(ctx: &mut ProgramTestContext, ms: &Pubkey) -> Multisig {
    let acct = ctx.banks_client.get_account(*ms).await.unwrap().unwrap();
    Multisig::try_from_slice(&acct.data).unwrap()
}

async fn fetch_proposal(ctx: &mut ProgramTestContext, p: &Pubkey) -> Proposal {
    let acct = ctx.banks_client.get_account(*p).await.unwrap().unwrap();
    let mut data: &[u8] = &acct.data;
    Proposal::deserialize(&mut data).unwrap()
}

async fn sol_balance(ctx: &mut ProgramTestContext, acct: &Pubkey) -> u64 {
    ctx.banks_client.get_balance(*acct).await.unwrap()
}

/// Test fixture: 2-of-3 multisig with funded vault.
struct Fixture {
    pid: Pubkey,
    multisig: Pubkey,
    vault: Pubkey,
    owners: Vec<Keypair>,
}

impl Fixture {
    async fn new(ctx: &mut ProgramTestContext, pid: Pubkey, timelock: u64) -> Self {
        let owners: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
        for o in &owners {
            fund(ctx, &o.pubkey(), 10_000_000_000).await;
        }
        let owner_pks: Vec<Pubkey> = owners.iter().map(|k| k.pubkey()).collect();
        let creator = &owners[0];
        let (ix, ms, vault) =
            ix_create_multisig(&pid, &creator.pubkey(), owner_pks, 2, timelock, 7);
        send_tx(ctx, &[ix], &[creator]).await.unwrap();
        let dep = ix_deposit(&pid, &ctx.payer.pubkey(), &vault, 10_000_000_000);
        send_tx(ctx, &[dep], &[]).await.unwrap();
        Self {
            pid,
            multisig: ms,
            vault,
            owners,
        }
    }

    async fn propose_sol(
        &self,
        ctx: &mut ProgramTestContext,
        proposer_idx: usize,
        destination: Pubkey,
        amount: u64,
    ) -> (Pubkey, u64) {
        let ms = fetch_multisig(ctx, &self.multisig).await;
        let index = ms.proposal_count;
        let (ix, prop) = ix_propose(
            &self.pid,
            &self.owners[proposer_idx].pubkey(),
            &self.multisig,
            index,
            ProposalKind::SolTransfer,
            destination,
            amount,
            Pubkey::default(),
        );
        send_tx(ctx, &[ix], &[&self.owners[proposer_idx]])
            .await
            .unwrap();
        (prop, index)
    }
}

#[tokio::test]
async fn test_create_multisig() {
    let (mut ctx, pid) = start_context().await;
    let owners: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
    for o in &owners {
        fund(&mut ctx, &o.pubkey(), 10_000_000_000).await;
    }
    let owner_pks: Vec<Pubkey> = owners.iter().map(|k| k.pubkey()).collect();
    let (ix, ms, vault) =
        ix_create_multisig(&pid, &owners[0].pubkey(), owner_pks.clone(), 2, 3600, 1);
    send_tx(&mut ctx, &[ix], &[&owners[0]]).await.unwrap();

    let state = fetch_multisig(&mut ctx, &ms).await;
    assert_eq!(state.owner_count, 3);
    assert_eq!(state.threshold, 2);
    assert_eq!(state.timelock_seconds, 3600);
    assert_eq!(state.proposal_count, 0);
    assert_eq!(state.nonce, 1);
    assert_eq!(state.creator, owners[0].pubkey());
    for (i, pk) in owner_pks.iter().enumerate() {
        assert_eq!(state.owners[i], *pk);
    }
    let (expected_ms, _) = multisig_pda(&owners[0].pubkey(), 1, &pid);
    assert_eq!(ms, expected_ms);
    let (expected_vault, _) = vault_pda(&ms, &pid);
    assert_eq!(vault, expected_vault);
    let vault_acct = ctx.banks_client.get_account(vault).await.unwrap().unwrap();
    assert_eq!(vault_acct.owner, pid);
}

#[tokio::test]
async fn test_create_multisig_rejects_bad_config() {
    let (mut ctx, pid) = start_context().await;
    let owners: Vec<Keypair> = (0..2).map(|_| Keypair::new()).collect();
    for o in &owners {
        fund(&mut ctx, &o.pubkey(), 10_000_000_000).await;
    }
    let pks: Vec<Pubkey> = owners.iter().map(|k| k.pubkey()).collect();

    // threshold 0
    let (ix, _, _) = ix_create_multisig(&pid, &pks[0], pks.clone(), 0, 60, 1);
    assert!(send_tx(&mut ctx, &[ix], &[&owners[0]]).await.is_err());
    // threshold > owners
    let (ix, _, _) = ix_create_multisig(&pid, &pks[0], pks.clone(), 3, 60, 2);
    assert!(send_tx(&mut ctx, &[ix], &[&owners[0]]).await.is_err());
    // duplicate owner
    let (ix, _, _) = ix_create_multisig(&pid, &pks[0], vec![pks[0], pks[0]], 2, 60, 3);
    assert!(send_tx(&mut ctx, &[ix], &[&owners[0]]).await.is_err());
    // no owners
    let (ix, _, _) = ix_create_multisig(&pid, &pks[0], vec![], 1, 60, 4);
    assert!(send_tx(&mut ctx, &[ix], &[&owners[0]]).await.is_err());
}

#[tokio::test]
async fn test_propose_and_approve_to_threshold() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 3600).await;
    let dest = Pubkey::new_unique();
    let (prop, _) = f.propose_sol(&mut ctx, 0, dest, 1_000_000).await;

    let p = fetch_proposal(&mut ctx, &prop).await;
    assert_eq!(p.approval_count, 1);
    assert!(p.has_approved(&f.owners[0].pubkey()));

    // second owner approves -> threshold reached
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.unwrap();
    let p = fetch_proposal(&mut ctx, &prop).await;
    assert_eq!(p.approval_count, 2);

    // double approve fails
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    assert!(send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.is_err());

    // non-owner cannot approve
    let stranger = Keypair::new();
    let ix = ix_approve(&pid, &stranger.pubkey(), &f.multisig, &prop);
    assert!(send_tx(&mut ctx, &[ix], &[&stranger]).await.is_err());
}

#[tokio::test]
async fn test_timelock_blocks_early_execution() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 3600).await;
    let dest = Pubkey::new_unique();
    let (prop, _) = f.propose_sol(&mut ctx, 0, dest, 1_000_000).await;
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.unwrap();

    // threshold reached but timelock (1h) not expired -> fails
    let before = sol_balance(&mut ctx, &dest).await;
    let ix = ix_execute_sol(&pid, &f.owners[0].pubkey(), &f.multisig, &prop, &f.vault, &dest);
    assert!(send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.is_err());
    assert_eq!(sol_balance(&mut ctx, &dest).await, before);
}

#[tokio::test]
async fn test_execute_sol_after_timelock() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 120).await;
    let dest = Pubkey::new_unique();
    let (prop, _) = f.propose_sol(&mut ctx, 0, dest, 2_500_000).await;
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.unwrap();

    let p = fetch_proposal(&mut ctx, &prop).await;
    let now = clock_now(&mut ctx).await;
    assert!(now < p.created_at + 120);
    warp_to(&mut ctx, p.created_at + 120).await;

    let vault_before = sol_balance(&mut ctx, &f.vault).await;
    let ix = ix_execute_sol(&pid, &f.owners[2].pubkey(), &f.multisig, &prop, &f.vault, &dest);
    send_tx(&mut ctx, &[ix], &[&f.owners[2]]).await.unwrap();

    assert_eq!(sol_balance(&mut ctx, &dest).await, 2_500_000);
    assert_eq!(sol_balance(&mut ctx, &f.vault).await, vault_before - 2_500_000);
    let p = fetch_proposal(&mut ctx, &prop).await;
    assert!(p.executed);

    // executing twice fails
    let ix = ix_execute_sol(&pid, &f.owners[0].pubkey(), &f.multisig, &prop, &f.vault, &dest);
    assert!(send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.is_err());
}

#[tokio::test]
async fn test_cancel_by_proposer() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 60).await;
    let dest = Pubkey::new_unique();
    let (prop, _) = f.propose_sol(&mut ctx, 0, dest, 1_000_000).await;

    let ix = ix_cancel(&pid, &f.owners[0].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    let p = fetch_proposal(&mut ctx, &prop).await;
    assert!(p.cancelled);

    // cancelled proposal cannot be approved or executed
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    assert!(send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.is_err());
    let later = clock_now(&mut ctx).await + 3600;
    warp_to(&mut ctx, later).await;
    let ix = ix_execute_sol(&pid, &f.owners[1].pubkey(), &f.multisig, &prop, &f.vault, &dest);
    assert!(send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.is_err());
}

#[tokio::test]
async fn test_cancel_by_owner_votes() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 60).await;
    let dest = Pubkey::new_unique();
    // owner[1] proposes; owner[1] is the proposer, so owner[0] and owner[2]
    // must vote to cancel (threshold 2)
    let (prop, _) = f.propose_sol(&mut ctx, 1, dest, 1_000_000).await;

    let ix = ix_cancel(&pid, &f.owners[0].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    let p = fetch_proposal(&mut ctx, &prop).await;
    assert!(!p.cancelled);
    assert_eq!(p.cancel_count, 1);

    let ix = ix_cancel(&pid, &f.owners[2].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[2]]).await.unwrap();
    let p = fetch_proposal(&mut ctx, &prop).await;
    assert!(p.cancelled);
}

#[tokio::test]
async fn test_owner_management_via_proposal() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 30).await;

    // add a fourth owner
    let newcomer = Pubkey::new_unique();
    let ms = fetch_multisig(&mut ctx, &f.multisig).await;
    let (ix, prop) = ix_propose(
        &pid,
        &f.owners[0].pubkey(),
        &f.multisig,
        ms.proposal_count,
        ProposalKind::AddOwner { new_owner: newcomer },
        Pubkey::default(),
        0,
        Pubkey::default(),
    );
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.unwrap();
    let p = fetch_proposal(&mut ctx, &prop).await;
    warp_to(&mut ctx, p.created_at + 30).await;
    let ix = ix_execute_config(&pid, &f.owners[2].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[2]]).await.unwrap();
    let ms = fetch_multisig(&mut ctx, &f.multisig).await;
    assert_eq!(ms.owner_count, 4);
    assert!(ms.is_owner(&newcomer));

    // raise threshold to 3
    let (ix, prop) = ix_propose(
        &pid,
        &f.owners[0].pubkey(),
        &f.multisig,
        ms.proposal_count,
        ProposalKind::ChangeThreshold { new_threshold: 3 },
        Pubkey::default(),
        0,
        Pubkey::default(),
    );
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    for i in 1..3 {
        let ix = ix_approve(&pid, &f.owners[i].pubkey(), &f.multisig, &prop);
        send_tx(&mut ctx, &[ix], &[&f.owners[i]]).await.unwrap();
    }
    let p = fetch_proposal(&mut ctx, &prop).await;
    warp_to(&mut ctx, p.created_at + 30).await;
    let ix = ix_execute_config(&pid, &f.owners[0].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    let ms = fetch_multisig(&mut ctx, &f.multisig).await;
    assert_eq!(ms.threshold, 3);

    // removing an owner now would drop to 3 owners with threshold 3;
    // removing one more would break it, so the second removal must fail.
    // first removal: 4 -> 3 owners, threshold 3: allowed (3 >= 3)
    let (ix, prop) = ix_propose(
        &pid,
        &f.owners[0].pubkey(),
        &f.multisig,
        ms.proposal_count,
        ProposalKind::RemoveOwner { owner: newcomer },
        Pubkey::default(),
        0,
        Pubkey::default(),
    );
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    for i in 1..3 {
        let ix = ix_approve(&pid, &f.owners[i].pubkey(), &f.multisig, &prop);
        send_tx(&mut ctx, &[ix], &[&f.owners[i]]).await.unwrap();
    }
    let p = fetch_proposal(&mut ctx, &prop).await;
    warp_to(&mut ctx, p.created_at + 30).await;
    let ix = ix_execute_config(&pid, &f.owners[0].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    let ms = fetch_multisig(&mut ctx, &f.multisig).await;
    assert_eq!(ms.owner_count, 3);
    assert!(!ms.is_owner(&newcomer));

    // now 3 owners, threshold 3: removing anyone would break threshold
    let (ix, prop) = ix_propose(
        &pid,
        &f.owners[0].pubkey(),
        &f.multisig,
        ms.proposal_count,
        ProposalKind::RemoveOwner {
            owner: f.owners[2].pubkey(),
        },
        Pubkey::default(),
        0,
        Pubkey::default(),
    );
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    for i in 1..3 {
        let ix = ix_approve(&pid, &f.owners[i].pubkey(), &f.multisig, &prop);
        send_tx(&mut ctx, &[ix], &[&f.owners[i]]).await.unwrap();
    }
    let p = fetch_proposal(&mut ctx, &prop).await;
    warp_to(&mut ctx, p.created_at + 30).await;
    let ix = ix_execute_config(&pid, &f.owners[0].pubkey(), &f.multisig, &prop);
    assert!(send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.is_err());
}

#[tokio::test]
async fn test_token_transfer_proposal() {
    let (mut ctx, pid) = start_context().await;
    let f = Fixture::new(&mut ctx, pid, 30).await;

    // create mint + vault token account + destination token account
    let mint = Keypair::new();
    let vault_token = Keypair::new();
    let dest_token = Keypair::new();
    let dest_owner = Pubkey::new_unique();
    create_mint(&mut ctx, &mint).await;
    create_token_account(&mut ctx, &vault_token, &mint.pubkey(), &f.vault).await;
    create_token_account(&mut ctx, &dest_token, &mint.pubkey(), &dest_owner).await;
    mint_to(&mut ctx, &mint, &vault_token.pubkey(), 1_000_000).await;

    let ms = fetch_multisig(&mut ctx, &f.multisig).await;
    let (ix, prop) = ix_propose(
        &pid,
        &f.owners[0].pubkey(),
        &f.multisig,
        ms.proposal_count,
        ProposalKind::TokenTransfer,
        dest_token.pubkey(),
        400_000,
        mint.pubkey(),
    );
    send_tx(&mut ctx, &[ix], &[&f.owners[0]]).await.unwrap();
    let ix = ix_approve(&pid, &f.owners[1].pubkey(), &f.multisig, &prop);
    send_tx(&mut ctx, &[ix], &[&f.owners[1]]).await.unwrap();
    let p = fetch_proposal(&mut ctx, &prop).await;
    warp_to(&mut ctx, p.created_at + 30).await;

    let ix = Instruction {
        program_id: pid,
        accounts: vec![
            AccountMeta::new_readonly(f.owners[2].pubkey(), true),
            AccountMeta::new(f.multisig, false),
            AccountMeta::new(prop, false),
            AccountMeta::new_readonly(CLOCK_SYSVAR_ID, false),
            AccountMeta::new_readonly(f.vault, false),
            AccountMeta::new(vault_token.pubkey(), false),
            AccountMeta::new(dest_token.pubkey(), false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data: TreasuryInstruction::Execute.pack(),
    };
    send_tx(&mut ctx, &[ix], &[&f.owners[2]]).await.unwrap();

    assert_eq!(token_balance(&mut ctx, &dest_token.pubkey()).await, 400_000);
    assert_eq!(
        token_balance(&mut ctx, &vault_token.pubkey()).await,
        600_000
    );
}

// ---------- SPL helpers ----------

async fn create_mint(ctx: &mut ProgramTestContext, mint: &Keypair) {
    let rent = ctx.banks_client.get_rent().await.unwrap();
    let space = 82usize;
    let lamports = rent.minimum_balance(space);
    let ix = Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(ctx.payer.pubkey(), true),
            AccountMeta::new(mint.pubkey(), true),
        ],
        data: {
            let mut d = Vec::with_capacity(52);
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&lamports.to_le_bytes());
            d.extend_from_slice(&(space as u64).to_le_bytes());
            d.extend_from_slice(spl_token::id().as_ref());
            d
        },
    };
    let recent = ctx.banks_client.get_latest_blockhash().await.unwrap();
    let msg = Message::new_with_blockhash(&[ix], Some(&ctx.payer.pubkey()), &recent);
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(&[&ctx.payer, mint], recent);
    ctx.banks_client.process_transaction(tx).await.unwrap();

    let data = spl_token::instruction::initialize_mint(
        &spl_token::id(),
        &mint.pubkey(),
        &ctx.payer.pubkey(),
        None,
        6,
    )
    .unwrap();
    let ix = Instruction {
        program_id: spl_token::id(),
        accounts: data.accounts,
        data: data.data,
    };
    send_tx(ctx, &[ix], &[]).await.unwrap();
}

async fn create_token_account(
    ctx: &mut ProgramTestContext,
    acct: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
) {
    let rent = ctx.banks_client.get_rent().await.unwrap();
    let space = 165usize;
    let lamports = rent.minimum_balance(space);
    let ix = Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(ctx.payer.pubkey(), true),
            AccountMeta::new(acct.pubkey(), true),
        ],
        data: {
            let mut d = Vec::with_capacity(52);
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&lamports.to_le_bytes());
            d.extend_from_slice(&(space as u64).to_le_bytes());
            d.extend_from_slice(spl_token::id().as_ref());
            d
        },
    };
    let recent = ctx.banks_client.get_latest_blockhash().await.unwrap();
    let msg = Message::new_with_blockhash(&[ix], Some(&ctx.payer.pubkey()), &recent);
    let mut tx = Transaction::new_unsigned(msg);
    tx.sign(&[&ctx.payer, acct], recent);
    ctx.banks_client.process_transaction(tx).await.unwrap();

    let data = spl_token::instruction::initialize_account(
        &spl_token::id(),
        &acct.pubkey(),
        mint,
        owner,
    )
    .unwrap();
    let ix = Instruction {
        program_id: spl_token::id(),
        accounts: data.accounts,
        data: data.data,
    };
    send_tx(ctx, &[ix], &[]).await.unwrap();
}

async fn mint_to(
    ctx: &mut ProgramTestContext,
    mint: &Keypair,
    dest: &Pubkey,
    amount: u64,
) {
    let data =
        spl_token::instruction::mint_to(&spl_token::id(), &mint.pubkey(), dest, &ctx.payer.pubkey(), &[], amount)
            .unwrap();
    let ix = Instruction {
        program_id: spl_token::id(),
        accounts: data.accounts,
        data: data.data,
    };
    send_tx(ctx, &[ix], &[]).await.unwrap();
}

async fn token_balance(ctx: &mut ProgramTestContext, acct: &Pubkey) -> u64 {
    let data = ctx
        .banks_client
        .get_account(*acct)
        .await
        .unwrap()
        .unwrap()
        .data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}
