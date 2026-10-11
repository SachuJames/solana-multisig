use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{program_error::ProgramError, pubkey::Pubkey};

use crate::state::ProposalKind;

/// Arguments for [`TreasuryInstruction::CreateMultisig`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CreateMultisigArgs {
    pub owners: Vec<Pubkey>,
    pub threshold: u8,
    pub timelock_seconds: u64,
    pub nonce: u64,
}

/// Arguments for [`TreasuryInstruction::Propose`].
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProposeArgs {
    pub kind: ProposalKind,
    pub destination: Pubkey,
    pub amount: u64,
    pub mint: Pubkey,
}

/// Instructions for the multisig treasury program.
pub enum TreasuryInstruction {
    /// Create a multisig and its SOL vault.
    ///
    /// Accounts:
    /// 0. `[signer, writable]` creator (payer)
    /// 1. `[writable]` multisig PDA `[b"multisig", creator, nonce]`
    /// 2. `[writable]` vault PDA `[b"vault", multisig]`
    /// 3. `[]` system program
    CreateMultisig(CreateMultisigArgs),
    /// Create a proposal. The proposer counts as the first approver.
    ///
    /// Accounts:
    /// 0. `[signer, writable]` proposer (must be an owner; pays for the proposal account)
    /// 1. `[writable]` multisig
    /// 2. `[writable]` proposal PDA `[b"proposal", multisig, index]`
    /// 3. `[]` system program
    /// 4. `[]` clock sysvar
    Propose(ProposeArgs),
    /// Approve a proposal. Executes nothing by itself.
    ///
    /// Accounts:
    /// 0. `[signer]` owner
    /// 1. `[]` multisig
    /// 2. `[writable]` proposal
    Approve,
    /// Execute a proposal that reached threshold and whose timelock expired.
    /// Anyone may call this.
    ///
    /// Common accounts:
    /// 0. `[signer]` executor (pays nothing, only a signer for ordering)
    /// 1. `[writable]` multisig
    /// 2. `[writable]` proposal
    /// 3. `[]` clock sysvar
    ///
    /// SOL transfer additionally:
    /// 4. `[writable]` vault PDA
    /// 5. `[writable]` destination
    /// 6. `[]` system program
    ///
    /// Token transfer additionally:
    /// 4. `[]` vault PDA (signs the token CPI)
    /// 5. `[writable]` vault token account (owner = vault PDA)
    /// 6. `[writable]` destination token account
    /// 7. `[]` SPL Token program
    Execute,
    /// Vote to cancel a proposal. The proposer cancels immediately;
    /// other owners need `threshold` cancel votes.
    ///
    /// Accounts:
    /// 0. `[signer]` owner
    /// 1. `[]` multisig
    /// 2. `[writable]` proposal
    Cancel,
    /// Fund the SOL vault with `amount` lamports.
    ///
    /// Accounts:
    /// 0. `[signer, writable]` funder
    /// 1. `[writable]` vault PDA
    /// 2. `[]` system program
    Deposit { amount: u64 },
}

impl TreasuryInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input
            .split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;
        Ok(match tag {
            0 => Self::CreateMultisig(
                CreateMultisigArgs::try_from_slice(rest)
                    .map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            1 => Self::Propose(
                ProposeArgs::try_from_slice(rest)
                    .map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            2 => Self::Approve,
            3 => Self::Execute,
            4 => Self::Cancel,
            5 => {
                if rest.len() != 8 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                let amount = u64::from_le_bytes(rest[..8].try_into().unwrap());
                Self::Deposit { amount }
            }
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }

    pub fn pack(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::CreateMultisig(a) => {
                out.push(0);
                out.extend_from_slice(&borsh::to_vec(a).unwrap());
            }
            Self::Propose(a) => {
                out.push(1);
                out.extend_from_slice(&borsh::to_vec(a).unwrap());
            }
            Self::Approve => out.push(2),
            Self::Execute => out.push(3),
            Self::Cancel => out.push(4),
            Self::Deposit { amount } => {
                out.push(5);
                out.extend_from_slice(&amount.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let ix = TreasuryInstruction::CreateMultisig(CreateMultisigArgs {
            owners: vec![Pubkey::new_unique(), Pubkey::new_unique()],
            threshold: 2,
            timelock_seconds: 3600,
            nonce: 42,
        });
        let packed = ix.pack();
        let back = TreasuryInstruction::unpack(&packed).unwrap();
        assert!(matches!(back, TreasuryInstruction::CreateMultisig(_)));

        for (tag, expect) in [(2u8, "Approve"), (3, "Execute"), (4, "Cancel")] {
            let back = TreasuryInstruction::unpack(&[tag]).unwrap();
            let name = match back {
                TreasuryInstruction::Approve => "Approve",
                TreasuryInstruction::Execute => "Execute",
                TreasuryInstruction::Cancel => "Cancel",
                _ => "other",
            };
            assert_eq!(name, expect);
        }
        let dep = TreasuryInstruction::Deposit { amount: 500 };
        let back = TreasuryInstruction::unpack(&dep.pack()).unwrap();
        assert!(matches!(
            back,
            TreasuryInstruction::Deposit { amount: 500 }
        ));
        assert!(TreasuryInstruction::unpack(&[5]).is_err());
        assert!(TreasuryInstruction::unpack(&[9]).is_err());
        assert!(TreasuryInstruction::unpack(&[]).is_err());
    }
}
