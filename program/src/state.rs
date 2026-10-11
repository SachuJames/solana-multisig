use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;

use crate::error::TreasuryError;
use solana_program::program_error::ProgramError;

/// Maximum owners per multisig. Fixed so account sizes are constant.
pub const MAX_OWNERS: usize = 8;

/// Exact serialized size of [`Multisig`].
pub const MULTISIG_LEN: usize = 32 + 32 * MAX_OWNERS + 1 + 1 + 8 + 8 + 8 + 1;

/// Exact serialized size of [`Proposal`].
pub const PROPOSAL_LEN: usize =
    32 + 8 + 32 + PROPOSAL_KIND_LEN + 32 + 8 + 32 + 8 + 1 + 1 + 1 + 32 * MAX_OWNERS + 1 + 32 * MAX_OWNERS + 1;

/// Serialized size of [`ProposalKind`]: 1-byte tag + largest variant (Pubkey).
pub const PROPOSAL_KIND_LEN: usize = 1 + 32;

/// The multisig configuration account.
///
/// PDA seeds: `[b"multisig", creator_pubkey, nonce.to_le_bytes()]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Multisig {
    /// Account that created the multisig (informational).
    pub creator: Pubkey,
    /// Owner set. Only the first `owner_count` entries are live.
    pub owners: [Pubkey; MAX_OWNERS],
    /// Number of live owners.
    pub owner_count: u8,
    /// Approvals required to execute a proposal.
    pub threshold: u8,
    /// Seconds a proposal waits after reaching threshold before execution.
    pub timelock_seconds: u64,
    /// Total proposals ever created (also the next proposal index).
    pub proposal_count: u64,
    /// Creation nonce, part of the PDA seeds.
    pub nonce: u64,
    pub bump: u8,
}

impl Multisig {
    /// Position of `owner` in the owner set, if present.
    pub fn owner_index(&self, owner: &Pubkey) -> Option<usize> {
        self.owners[..self.owner_count as usize]
            .iter()
            .position(|o| o == owner)
    }

    pub fn is_owner(&self, owner: &Pubkey) -> bool {
        self.owner_index(owner).is_some()
    }
}

/// What a proposal does when executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ProposalKind {
    /// Move `amount` lamports of SOL from the vault to `destination`.
    SolTransfer,
    /// Move `amount` tokens of `mint` from the vault token account.
    TokenTransfer,
    /// Add a new owner to the multisig.
    AddOwner { new_owner: Pubkey },
    /// Remove an owner from the multisig.
    RemoveOwner { owner: Pubkey },
    /// Change the approval threshold.
    ChangeThreshold { new_threshold: u8 },
    /// Change the timelock duration.
    ChangeTimelock { new_timelock: u64 },
}

/// A single spending or configuration change proposal.
///
/// PDA seeds: `[b"proposal", multisig_pubkey, index.to_le_bytes()]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    /// The multisig this proposal belongs to.
    pub multisig: Pubkey,
    /// Sequential index within the multisig.
    pub index: u64,
    /// Owner that created the proposal.
    pub proposer: Pubkey,
    /// What executing this proposal does.
    pub kind: ProposalKind,
    /// Where funds go (for transfer kinds).
    pub destination: Pubkey,
    /// Lamports or token amount (for transfer kinds).
    pub amount: u64,
    /// SPL mint (for token transfers; default pubkey otherwise).
    pub mint: Pubkey,
    /// Unix timestamp of creation.
    pub created_at: i64,
    /// Set once executed.
    pub executed: bool,
    /// Set once cancelled.
    pub cancelled: bool,
    /// Number of live approvals.
    pub approval_count: u8,
    /// Owners that approved. First `approval_count` entries are live.
    pub approvers: [Pubkey; MAX_OWNERS],
    /// Number of live cancel votes.
    pub cancel_count: u8,
    /// Owners that voted to cancel.
    pub cancel_approvers: [Pubkey; MAX_OWNERS],
    pub bump: u8,
}

impl ProposalKind {
    pub fn is_transfer_kind(&self) -> bool {
        matches!(
            self,
            ProposalKind::SolTransfer | ProposalKind::TokenTransfer
        )
    }
}

impl Proposal {
    pub fn has_approved(&self, owner: &Pubkey) -> bool {
        self.approvers[..self.approval_count as usize]
            .iter()
            .any(|o| o == owner)
    }

    pub fn has_voted_cancel(&self, owner: &Pubkey) -> bool {
        self.cancel_approvers[..self.cancel_count as usize]
            .iter()
            .any(|o| o == owner)
    }

    /// Earliest unix timestamp at which execution is allowed.
    pub fn executable_at(&self, timelock_seconds: u64) -> Result<i64, ProgramError> {
        self.created_at
            .checked_add(timelock_seconds as i64)
            .ok_or(TreasuryError::ArithmeticOverflow.into())
    }

    pub fn is_transfer(&self) -> bool {
        matches!(
            self.kind,
            ProposalKind::SolTransfer | ProposalKind::TokenTransfer
        )
    }
}

/// Minimal parse of an SPL Token account: (mint, owner, amount).
pub fn parse_token_account(data: &[u8]) -> Result<(Pubkey, Pubkey, u64), ProgramError> {
    if data.len() < 72 {
        return Err(ProgramError::InvalidAccountData);
    }
    let mint = Pubkey::new_from_array(data[0..32].try_into().unwrap());
    let owner = Pubkey::new_from_array(data[32..64].try_into().unwrap());
    let amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
    Ok((mint, owner, amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_sizes_match_constants() {
        let ms = Multisig {
            creator: Pubkey::new_unique(),
            owners: [Pubkey::default(); MAX_OWNERS],
            owner_count: 3,
            threshold: 2,
            timelock_seconds: 60,
            proposal_count: 0,
            nonce: 7,
            bump: 255,
        };
        assert_eq!(borsh::to_vec(&ms).unwrap().len(), MULTISIG_LEN);

        let p = Proposal {
            multisig: Pubkey::new_unique(),
            index: 0,
            proposer: Pubkey::new_unique(),
            kind: ProposalKind::AddOwner {
                new_owner: Pubkey::new_unique(),
            },
            destination: Pubkey::default(),
            amount: 0,
            mint: Pubkey::default(),
            created_at: 1_700_000_000,
            executed: false,
            cancelled: false,
            approval_count: 0,
            approvers: [Pubkey::default(); MAX_OWNERS],
            cancel_count: 0,
            cancel_approvers: [Pubkey::default(); MAX_OWNERS],
            bump: 255,
        };
        assert_eq!(borsh::to_vec(&p).unwrap().len(), PROPOSAL_LEN);
        assert_eq!(borsh::to_vec(&p.kind).unwrap().len(), PROPOSAL_KIND_LEN);
    }

    #[test]
    fn executable_at_adds_timelock() {
        let mut p = Proposal {
            multisig: Pubkey::default(),
            index: 0,
            proposer: Pubkey::default(),
            kind: ProposalKind::SolTransfer,
            destination: Pubkey::default(),
            amount: 0,
            mint: Pubkey::default(),
            created_at: 1000,
            executed: false,
            cancelled: false,
            approval_count: 0,
            approvers: [Pubkey::default(); MAX_OWNERS],
            cancel_count: 0,
            cancel_approvers: [Pubkey::default(); MAX_OWNERS],
            bump: 0,
        };
        assert_eq!(p.executable_at(60).unwrap(), 1060);
        p.created_at = i64::MAX;
        assert!(p.executable_at(60).is_err());
    }
}
