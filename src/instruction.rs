use solana_program::program_error::ProgramError;
use std::convert::TryInto;

use crate::error::EscrowError::InvalidInstruction;

pub enum EscrowInstruction {
    /// Starts the trade by creating and populating an escrow account and transferring ownership of the given temp token account to the PDA
    /// ("maker")
    ///
    ///
    /// Accounts expected:
    ///
    /// 0. `[signer]` The account of the person initializing the escrow
    /// 1. `[writable]` Temporary token account that should be created prior to this instruction and owned by the initializer
    /// 2. `[]` The token mint
    /// 3. `[writable]` The escrow account, it will hold all necessary info about the trade.
    /// 4. `[writable]` The account receiving the listing fee.
    /// 5. `[]` The system program
    /// 6. `[]` The rent sysvar
    /// 7. `[]` The token program
    InitEscrow {
        /// The SOL amount party A expects to receive, in Lamports
        amount: u64,
    },

    /// Accepts a trade ("taker")
    ///
    /// A sales fee is charged from the taker.
    ///
    /// If the taker is the same account that set up the escrow,
    /// no payment happens and no sales fee is charged, effectively
    /// allowing for a "Cancel Escrow" flow.
    ///
    /// Accounts expected:
    ///
    ///  0. `[signer]` The account of the person taking the trade
    ///  1. `[writable]` The taker's token account for the token they will receive should the trade go through
    ///  2. `[writable]` The PDA's temp token account to get tokens from and eventually close
    ///  3. `[writable]` The initializer's main account to send their rent fees to
    ///  4. `[writable]` The escrow account holding the escrow info
    ///  5. `[writable]` The account receiving sales fees.
    ///  6. `[writable]` Mint
    ///  7. `[writable]` Metadata account for the mint.
    ///     This MUST be the PDA with seeds ["metadata", METAPLEX_PROGRAM_ID, mint],
    ///     even if the mint does not have metadata.
    ///     If the account doesn't contain valid metadata, royalties will not be paid out.
    ///  8. `[]` The token program
    ///  9. `[]` The system program
    /// 10. `[]` The PDA account
    /// 11. `[]` Creator 0 account, if present in metadata, and in metadata order.
    /// XX. `[]` ...more creator accounts as above...
    Exchange {
        /// the amount the taker expects to be paid in the other token, as a u64 because that's the max possible supply of a token
        amount: u64,
    },
}

impl EscrowInstruction {
    /// Unpacks a byte buffer into a [EscrowInstruction](enum.EscrowInstruction.html).
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (tag, rest) = input.split_first().ok_or(InvalidInstruction)?;

        Ok(match tag {
            0 => Self::InitEscrow {
                amount: Self::unpack_amount(rest)?,
            },
            1 => Self::Exchange {
                amount: Self::unpack_amount(rest)?,
            },
            _ => return Err(InvalidInstruction.into()),
        })
    }

    fn unpack_amount(input: &[u8]) -> Result<u64, ProgramError> {
        let amount = input
            .get(..8)
            .and_then(|slice| slice.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or(InvalidInstruction)?;
        Ok(amount)
    }
}
