use borsh::BorshDeserialize;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::{pubkey, Pubkey},
    rent::Rent,
    sysvar::Sysvar,
};

use crate::{
    error::TreasuryError,
    instruction::{CreateMultisigArgs, ProposeArgs, TreasuryInstruction},
    state::{
        parse_token_account, Multisig, Proposal, ProposalKind, MAX_OWNERS, MULTISIG_LEN, PROPOSAL_LEN,
    },
};

/// System program id.
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

/// SPL Token program id.
pub const TOKEN_PROGRAM_ID: Pubkey =
    pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/// Clock sysvar id.
const CLOCK_SYSVAR_ID: Pubkey =
    pubkey!("SysvarC1ock11111111111111111111111111111111");

fn multisig_pda(creator: &Pubkey, nonce: u64, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"multisig", creator.as_ref(), &nonce.to_le_bytes()],
        program_id,
    )
}

fn vault_pda(multisig: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault", multisig.as_ref()], program_id)
}

fn proposal_pda(multisig: &Pubkey, index: u64, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"proposal", multisig.as_ref(), &index.to_le_bytes()],
        program_id,
    )
}

fn assert_pda(expected: &Pubkey, actual: &Pubkey) -> Result<(), ProgramError> {
    if expected != actual {
        return Err(TreasuryError::InvalidSeeds.into());
    }
    Ok(())
}

/// Unix timestamp from a clock sysvar account (bincode layout, field at [32..40]).
fn clock_timestamp(clock_info: &AccountInfo) -> Result<i64, ProgramError> {
    if clock_info.key != &CLOCK_SYSVAR_ID {
        return Err(ProgramError::InvalidArgument);
    }
    let data = clock_info.data.borrow();
    if data.len() < 40 {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(i64::from_le_bytes(data[32..40].try_into().unwrap()))
}

/// Build a system CreateAccount instruction (index 0).
fn system_create_account(
    from: &Pubkey,
    to: &Pubkey,
    lamports: u64,
    space: u64,
    owner: &Pubkey,
) -> Instruction {
    let mut data = Vec::with_capacity(4 + 8 + 8 + 32);
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(owner.as_ref());
    Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*from, true),
            AccountMeta::new(*to, true),
        ],
        data,
    }
}

/// Build a system Transfer instruction (index 2).
fn system_transfer(from: &Pubkey, to: &Pubkey, lamports: u64) -> Instruction {
    let mut data = Vec::with_capacity(4 + 8);
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&lamports.to_le_bytes());
    Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*from, true),
            AccountMeta::new(*to, false),
        ],
        data,
    }
}

/// Build an SPL Token Transfer instruction (index 3).
fn token_transfer_ix(
    source: &Pubkey,
    dest: &Pubkey,
    authority: &Pubkey,
    amount: u64,
) -> Instruction {
    let mut data = vec![3u8];
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*source, false),
            AccountMeta::new(*dest, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

fn create_pda<'a>(
    payer: &AccountInfo<'a>,
    new_account: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    program_id: &Pubkey,
    space: usize,
    seeds: &[&[u8]],
) -> ProgramResult {
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance(space);
    invoke_signed(
        &system_create_account(payer.key, new_account.key, lamports, space as u64, program_id),
        &[payer.clone(), new_account.clone(), system_program.clone()],
        &[seeds],
    )
}

fn load_multisig(data: &[u8]) -> Result<Multisig, ProgramError> {
    Multisig::try_from_slice(data).map_err(|_| ProgramError::InvalidAccountData.into())
}

fn load_proposal(data: &[u8]) -> Result<Proposal, ProgramError> {
    Proposal::try_from_slice(data).map_err(|_| ProgramError::InvalidAccountData.into())
}

fn save_multisig(ms: &Multisig, data: &mut [u8]) -> ProgramResult {
    let bytes = borsh::to_vec(ms).map_err(|_| ProgramError::InvalidAccountData)?;
    if bytes.len() != MULTISIG_LEN || data.len() < MULTISIG_LEN {
        return Err(ProgramError::AccountDataTooSmall);
    }
    data[..MULTISIG_LEN].copy_from_slice(&bytes);
    Ok(())
}

fn save_proposal(p: &Proposal, data: &mut [u8]) -> ProgramResult {
    let bytes = borsh::to_vec(p).map_err(|_| ProgramError::InvalidAccountData)?;
    if bytes.len() != PROPOSAL_LEN || data.len() < PROPOSAL_LEN {
        return Err(ProgramError::AccountDataTooSmall);
    }
    data[..PROPOSAL_LEN].copy_from_slice(&bytes);
    Ok(())
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    input: &[u8],
) -> ProgramResult {
    match TreasuryInstruction::unpack(input)? {
        TreasuryInstruction::CreateMultisig(args) => create_multisig(program_id, accounts, args),
        TreasuryInstruction::Propose(args) => propose(program_id, accounts, args),
        TreasuryInstruction::Approve => approve(program_id, accounts),
        TreasuryInstruction::Execute => execute(program_id, accounts),
        TreasuryInstruction::Cancel => cancel(program_id, accounts),
        TreasuryInstruction::Deposit { amount } => deposit(program_id, accounts, amount),
    }
}

fn create_multisig(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: CreateMultisigArgs,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let creator = next_account_info(it)?;
    let multisig_ai = next_account_info(it)?;
    let vault_ai = next_account_info(it)?;
    let system_program = next_account_info(it)?;

    if !creator.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if *system_program.key != SYSTEM_PROGRAM_ID {
        return Err(TreasuryError::InvalidSystemProgram.into());
    }
    if args.owners.is_empty() {
        return Err(TreasuryError::NoOwners.into());
    }
    if args.owners.len() > MAX_OWNERS {
        return Err(TreasuryError::TooManyOwners.into());
    }
    if args.threshold == 0 || args.threshold as usize > args.owners.len() {
        return Err(TreasuryError::InvalidThreshold.into());
    }
    let mut seen: Vec<&Pubkey> = Vec::with_capacity(args.owners.len());
    for o in &args.owners {
        if seen.contains(&o) {
            return Err(TreasuryError::DuplicateOwner.into());
        }
        seen.push(o);
    }

    let (ms_key, ms_bump) = multisig_pda(creator.key, args.nonce, program_id);
    assert_pda(&ms_key, multisig_ai.key)?;
    let (vault_key, vault_bump) = vault_pda(&ms_key, program_id);
    assert_pda(&vault_key, vault_ai.key)?;
    if multisig_ai.data_len() != 0 || vault_ai.data_len() != 0 {
        return Err(TreasuryError::AlreadyInitialized.into());
    }

    let nonce_le = args.nonce.to_le_bytes();
    let ms_seeds: &[&[u8]] = &[
        b"multisig",
        creator.key.as_ref(),
        &nonce_le,
        &[ms_bump],
    ];
    create_pda(
        creator,
        multisig_ai,
        system_program,
        program_id,
        MULTISIG_LEN,
        ms_seeds,
    )?;
    let vault_seeds: &[&[u8]] = &[b"vault", ms_key.as_ref(), &[vault_bump]];
    create_pda(
        creator,
        vault_ai,
        system_program,
        program_id,
        0,
        vault_seeds,
    )?;

    let mut owners = [Pubkey::default(); MAX_OWNERS];
    for (i, o) in args.owners.iter().enumerate() {
        owners[i] = *o;
    }
    let ms = Multisig {
        creator: *creator.key,
        owners,
        owner_count: args.owners.len() as u8,
        threshold: args.threshold,
        timelock_seconds: args.timelock_seconds,
        proposal_count: 0,
        nonce: args.nonce,
        bump: ms_bump,
    };
    save_multisig(&ms, &mut multisig_ai.data.borrow_mut())
}

fn propose(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    args: ProposeArgs,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let proposer = next_account_info(it)?;
    let multisig_ai = next_account_info(it)?;
    let proposal_ai = next_account_info(it)?;
    let system_program = next_account_info(it)?;
    let clock_ai = next_account_info(it)?;

    if !proposer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if multisig_ai.owner != program_id {
        return Err(ProgramError::IllegalOwner);
    }
    let mut ms = load_multisig(&multisig_ai.data.borrow())?;
    if !ms.is_owner(proposer.key) {
        return Err(TreasuryError::NotAnOwner.into());
    }
    if args.kind.is_transfer_kind() && args.amount == 0 {
        return Err(TreasuryError::InvalidAmount.into());
    }

    let index = ms.proposal_count;
    let (p_key, p_bump) = proposal_pda(multisig_ai.key, index, program_id);
    assert_pda(&p_key, proposal_ai.key)?;
    if proposal_ai.data_len() != 0 {
        return Err(TreasuryError::AlreadyInitialized.into());
    }

    let created_at = clock_timestamp(clock_ai)?;
    let mut approvers = [Pubkey::default(); MAX_OWNERS];
    approvers[0] = *proposer.key;
    let proposal = Proposal {
        multisig: *multisig_ai.key,
        index,
        proposer: *proposer.key,
        kind: args.kind,
        destination: args.destination,
        amount: args.amount,
        mint: args.mint,
        created_at,
        executed: false,
        cancelled: false,
        approval_count: 1,
        approvers,
        cancel_count: 0,
        cancel_approvers: [Pubkey::default(); MAX_OWNERS],
        bump: p_bump,
    };

    let index_le = index.to_le_bytes();
    let p_seeds: &[&[u8]] = &[
        b"proposal",
        multisig_ai.key.as_ref(),
        &index_le,
        &[p_bump],
    ];
    create_pda(
        proposer,
        proposal_ai,
        system_program,
        program_id,
        PROPOSAL_LEN,
        p_seeds,
    )?;
    save_proposal(&proposal, &mut proposal_ai.data.borrow_mut())?;

    ms.proposal_count = ms
        .proposal_count
        .checked_add(1)
        .ok_or(TreasuryError::ArithmeticOverflow)?;
    save_multisig(&ms, &mut multisig_ai.data.borrow_mut())
}

fn approve(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let owner = next_account_info(it)?;
    let multisig_ai = next_account_info(it)?;
    let proposal_ai = next_account_info(it)?;

    if !owner.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if multisig_ai.owner != program_id || proposal_ai.owner != program_id {
        return Err(ProgramError::IllegalOwner);
    }
    let ms = load_multisig(&multisig_ai.data.borrow())?;
    let mut p = load_proposal(&proposal_ai.data.borrow())?;
    if p.multisig != *multisig_ai.key {
        return Err(TreasuryError::InvalidSeeds.into());
    }
    if !ms.is_owner(owner.key) {
        return Err(TreasuryError::NotAnOwner.into());
    }
    if p.executed || p.cancelled {
        return Err(TreasuryError::InvalidProposalState.into());
    }
    if p.has_approved(owner.key) {
        return Err(TreasuryError::AlreadyApproved.into());
    }

    let slot = p.approval_count as usize;
    p.approvers[slot] = *owner.key;
    p.approval_count = p
        .approval_count
        .checked_add(1)
        .ok_or(TreasuryError::ArithmeticOverflow)?;
    save_proposal(&p, &mut proposal_ai.data.borrow_mut())
}

fn execute(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let executor = next_account_info(it)?;
    let multisig_ai = next_account_info(it)?;
    let proposal_ai = next_account_info(it)?;
    let clock_ai = next_account_info(it)?;

    if !executor.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if multisig_ai.owner != program_id || proposal_ai.owner != program_id {
        return Err(ProgramError::IllegalOwner);
    }
    let mut ms = load_multisig(&multisig_ai.data.borrow())?;
    let mut p = load_proposal(&proposal_ai.data.borrow())?;
    if p.multisig != *multisig_ai.key {
        return Err(TreasuryError::InvalidSeeds.into());
    }
    if p.executed {
        return Err(TreasuryError::AlreadyExecuted.into());
    }
    if p.cancelled {
        return Err(TreasuryError::ProposalCancelled.into());
    }
    if p.approval_count < ms.threshold {
        return Err(TreasuryError::ThresholdNotReached.into());
    }
    let now = clock_timestamp(clock_ai)?;
    if now < p.executable_at(ms.timelock_seconds)? {
        return Err(TreasuryError::TimelockNotExpired.into());
    }

    match p.kind {
        ProposalKind::SolTransfer => {
            let vault_ai = next_account_info(it)?;
            let dest_ai = next_account_info(it)?;
            let system_program = next_account_info(it)?;
            let (vault_key, _) = vault_pda(multisig_ai.key, program_id);
            assert_pda(&vault_key, vault_ai.key)?;
            if *dest_ai.key == vault_key {
                return Err(TreasuryError::InvalidDestination.into());
            }
            if *system_program.key != SYSTEM_PROGRAM_ID {
                return Err(TreasuryError::InvalidSystemProgram.into());
            }
            let remaining = vault_ai
                .lamports()
                .checked_sub(p.amount)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
            **vault_ai.lamports.borrow_mut() = remaining;
            **dest_ai.lamports.borrow_mut() = dest_ai
                .lamports()
                .checked_add(p.amount)
                .ok_or(TreasuryError::ArithmeticOverflow)?;
        }
        ProposalKind::TokenTransfer => {
            let vault_token_ai = next_account_info(it)?;
            let dest_token_ai = next_account_info(it)?;
            let token_program = next_account_info(it)?;
            let (vault_key, vault_bump) = vault_pda(multisig_ai.key, program_id);
            if *token_program.key != TOKEN_PROGRAM_ID {
                return Err(TreasuryError::InvalidTokenProgram.into());
            }
            let (v_mint, v_owner, v_amount) =
                parse_token_account(&vault_token_ai.data.borrow())?;
            if v_mint != p.mint || v_owner != vault_key {
                return Err(TreasuryError::InvalidTokenAccount.into());
            }
            if v_amount < p.amount {
                return Err(TreasuryError::ArithmeticOverflow.into());
            }
            let (d_mint, _, _) = parse_token_account(&dest_token_ai.data.borrow())?;
            if d_mint != p.mint {
                return Err(TreasuryError::InvalidTokenAccount.into());
            }
            let ms_key = *multisig_ai.key;
            let seeds: &[&[u8]] = &[b"vault", ms_key.as_ref(), &[vault_bump]];
            invoke_signed(
                &token_transfer_ix(
                    vault_token_ai.key,
                    dest_token_ai.key,
                    &vault_key,
                    p.amount,
                ),
                &[
                    vault_token_ai.clone(),
                    dest_token_ai.clone(),
                    token_program.clone(),
                ],
                &[seeds],
            )?;
        }
        ProposalKind::AddOwner { new_owner } => {
            if ms.owner_count as usize >= MAX_OWNERS {
                return Err(TreasuryError::TooManyOwners.into());
            }
            if ms.is_owner(&new_owner) {
                return Err(TreasuryError::DuplicateOwner.into());
            }
            ms.owners[ms.owner_count as usize] = new_owner;
            ms.owner_count += 1;
        }
        ProposalKind::RemoveOwner { owner } => {
            let idx = ms.owner_index(&owner).ok_or(TreasuryError::UnknownOwner)?;
            if ms.owner_count <= ms.threshold {
                return Err(TreasuryError::WouldBreakThreshold.into());
            }
            let last = ms.owner_count as usize - 1;
            ms.owners[idx] = ms.owners[last];
            ms.owners[last] = Pubkey::default();
            ms.owner_count -= 1;
        }
        ProposalKind::ChangeThreshold { new_threshold } => {
            if new_threshold == 0 || new_threshold > ms.owner_count {
                return Err(TreasuryError::InvalidThreshold.into());
            }
            ms.threshold = new_threshold;
        }
        ProposalKind::ChangeTimelock { new_timelock } => {
            ms.timelock_seconds = new_timelock;
        }
    }

    p.executed = true;
    save_proposal(&p, &mut proposal_ai.data.borrow_mut())?;
    save_multisig(&ms, &mut multisig_ai.data.borrow_mut())
}

fn cancel(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let owner = next_account_info(it)?;
    let multisig_ai = next_account_info(it)?;
    let proposal_ai = next_account_info(it)?;

    if !owner.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if multisig_ai.owner != program_id || proposal_ai.owner != program_id {
        return Err(ProgramError::IllegalOwner);
    }
    let ms = load_multisig(&multisig_ai.data.borrow())?;
    let mut p = load_proposal(&proposal_ai.data.borrow())?;
    if p.multisig != *multisig_ai.key {
        return Err(TreasuryError::InvalidSeeds.into());
    }
    if !ms.is_owner(owner.key) {
        return Err(TreasuryError::NotAnOwner.into());
    }
    if p.executed || p.cancelled {
        return Err(TreasuryError::InvalidProposalState.into());
    }

    if *owner.key == p.proposer {
        p.cancelled = true;
    } else {
        if p.has_voted_cancel(owner.key) {
            return Err(TreasuryError::AlreadyApproved.into());
        }
        let slot = p.cancel_count as usize;
        p.cancel_approvers[slot] = *owner.key;
        p.cancel_count = p
            .cancel_count
            .checked_add(1)
            .ok_or(TreasuryError::ArithmeticOverflow)?;
        if p.cancel_count >= ms.threshold {
            p.cancelled = true;
        }
    }
    save_proposal(&p, &mut proposal_ai.data.borrow_mut())
}

fn deposit(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let it = &mut accounts.iter();
    let funder = next_account_info(it)?;
    let vault_ai = next_account_info(it)?;
    let system_program = next_account_info(it)?;

    if !funder.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if vault_ai.owner != program_id {
        return Err(ProgramError::IllegalOwner);
    }
    if *system_program.key != SYSTEM_PROGRAM_ID {
        return Err(TreasuryError::InvalidSystemProgram.into());
    }
    if amount == 0 {
        return Err(TreasuryError::InvalidAmount.into());
    }
    invoke(
        &system_transfer(funder.key, vault_ai.key, amount),
        &[funder.clone(), vault_ai.clone(), system_program.clone()],
    )
}
