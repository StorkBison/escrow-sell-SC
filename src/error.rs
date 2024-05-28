use thiserror::Error;

use solana_program::program_error::ProgramError;

#[derive(Error, Debug, Copy, Clone)]
pub enum EscrowError {
    #[error("Invalid Instruction")]
    InvalidInstruction,

    #[error("Not Rent Exempt")]
    NotRentExempt,

    #[error("Expected Amount Mismatch")]
    ExpectedAmountMismatch,

    #[error("Amount Overflow")]
    AmountOverflow,

    #[error("Invalid sales tax recipient")]
    InvalidSalesTaxRecipient,

    #[error("Numeric Conversion Failed")]
    NumericConversionFailed,

    #[error("Invalid mint account")]
    InvalidMintAccount,

    #[error("Invalid token amount (needs to be exactly 1)")]
    InvalidTokenAmount,

    #[error("Invalid metadata")]
    InvalidMetadata,

    #[error("Missing metadata")]
    MissingMetadata,

    #[error("Invalid final amount")]
    InvalidFinalAmount,

    #[error("Royalty percentage too high")]
    InvalidRoyaltyFee,

    #[error("Creator mismatch")]
    CreatorMismatch,
}

impl From<EscrowError> for ProgramError {
    fn from(e: EscrowError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
