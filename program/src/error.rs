use solana_program::program_error::ProgramError;

/// Custom errors for the multisig treasury program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreasuryError {
    /// Need at least one owner.
    NoOwners = 0,
    /// Too many owners (max 8).
    TooManyOwners = 1,
    /// Threshold must be between 1 and the owner count.
    InvalidThreshold = 2,
    /// Duplicate owner in the owner set.
    DuplicateOwner = 3,
    /// Signer is not an owner of this multisig.
    NotAnOwner = 4,
    /// Proposal is not in a state that allows this action.
    InvalidProposalState = 5,
    /// Owner already approved this proposal.
    AlreadyApproved = 6,
    /// Proposal has not reached the approval threshold yet.
    ThresholdNotReached = 7,
    /// Timelock has not expired yet.
    TimelockNotExpired = 8,
    /// Proposal was already executed.
    AlreadyExecuted = 9,
    /// Proposal was cancelled.
    ProposalCancelled = 10,
    /// A PDA did not derive to the expected address.
    InvalidSeeds = 11,
    /// Expected the system program.
    InvalidSystemProgram = 12,
    /// Expected the SPL Token program.
    InvalidTokenProgram = 13,
    /// Token account failed validation (mint, owner or amount).
    InvalidTokenAccount = 14,
    /// Transfer amount must be greater than zero.
    InvalidAmount = 15,
    /// Checked arithmetic overflowed.
    ArithmeticOverflow = 16,
    /// Removing this owner would drop below the threshold.
    WouldBreakThreshold = 17,
    /// Owner is not part of the multisig.
    UnknownOwner = 18,
    /// Account is already initialized.
    AlreadyInitialized = 19,
    /// Proposal kind does not match the accounts given.
    InvalidProposalKind = 20,
    /// Destination must differ from the vault.
    InvalidDestination = 21,
}

impl From<TreasuryError> for ProgramError {
    fn from(e: TreasuryError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
