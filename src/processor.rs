use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack},
    pubkey::Pubkey,
    system_instruction,
    sysvar::{rent::Rent, Sysvar},
};

use spl_token::state::Account as TokenAccount;
use spl_token::state::Mint as TokenMint;

use crate::{error::EscrowError, instruction::EscrowInstruction, state::Escrow, metadata::Metadata, metadata::get_metadata_account};

const SALES_TAX_RECIPIENT_INTERNAL: &str = "8Ba7LXjBTWScPKMV4Lmz5dsenz53NVAwJsKYXyf7TzFZ";
const ESCROW_PDA_SEED: &[u8] = b"escrow";
const SALES_TAX: u64 = 250;
const LISTING_FEE: u64 = 10000000; // 0.01 SOL

pub struct Processor;
impl Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = EscrowInstruction::unpack(instruction_data)?;

        match instruction {
            EscrowInstruction::InitEscrow { amount } => {
                msg!("Instruction: InitEscrow");
                Self::process_init_escrow(accounts, amount, program_id)
            }
            EscrowInstruction::Exchange { amount } => {
                msg!("Instruction: Exchange");
                Self::process_exchange(accounts, amount, program_id)
            }
        }
    }

    fn process_init_escrow(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let initializer = next_account_info(account_info_iter)?;
        let temp_token_account = next_account_info(account_info_iter)?;
        let mint_account = next_account_info(account_info_iter)?;
        let escrow_account = next_account_info(account_info_iter)?;
        let sales_tax_recipient = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;
        let rent_account = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;

        if !initializer.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let mint = TokenMint::unpack(&mint_account.data.borrow())?;
        let spl_token_account = TokenAccount::unpack(&temp_token_account.data.borrow())?;
        if *mint_account.key != spl_token_account.mint {
            msg!("mint account mismatch: {:?} / {:?}", *mint_account.key, spl_token_account.mint);
            return Err(EscrowError::InvalidMintAccount.into());
        }
        if 10_usize.pow(mint.decimals as u32) != spl_token_account.amount as usize {
            // require token amount == 1
            msg!("invalid ui amount ({:?}/{:?})", spl_token_account.amount, mint.decimals);
            return Err(EscrowError::InvalidTokenAmount.into());
        }

        let rent = &Rent::from_account_info(rent_account)?;

        if !rent.is_exempt(escrow_account.lamports(), escrow_account.data_len()) {
            return Err(EscrowError::NotRentExempt.into());
        }

        let mut escrow_info = Escrow::unpack_unchecked(&escrow_account.data.borrow())?;
        if escrow_info.is_initialized() {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        let str_pk: &[u8] = &bs58::decode(SALES_TAX_RECIPIENT_INTERNAL).into_vec().expect("BUG decoding sales tax recipient!")[..];
        msg!("sales tax recipients: passed:{:?} / expected:{:?}", sales_tax_recipient.key, SALES_TAX_RECIPIENT_INTERNAL);
        if *sales_tax_recipient.key != Pubkey::new(str_pk) {
            msg!("Invalid sales tax recipient: {:?}", sales_tax_recipient);
            return Err(EscrowError::InvalidSalesTaxRecipient.into());
        }

        if LISTING_FEE > 0 {
            let xfer_listing_fee = system_instruction::transfer(&initializer.key, &sales_tax_recipient.key, LISTING_FEE);
            invoke(&xfer_listing_fee, &[initializer.clone(), sales_tax_recipient.clone(), system_program.clone()])?;
        }

        escrow_info.is_initialized = true;
        escrow_info.initializer_pubkey = *initializer.key;
        escrow_info.mint_pubkey = *mint_account.key;
        escrow_info.temp_token_account_pubkey = *temp_token_account.key;
        escrow_info.expected_amount = amount;

        Escrow::pack(escrow_info, &mut escrow_account.data.borrow_mut())?;
        let (pda, _nonce) = Pubkey::find_program_address(&[ESCROW_PDA_SEED], program_id);

        let owner_change_ix = spl_token::instruction::set_authority(
            token_program.key,
            temp_token_account.key,
            Some(&pda),
            spl_token::instruction::AuthorityType::AccountOwner,
            initializer.key,
            &[&initializer.key],
        )?;

        msg!("Calling the token program to transfer token account ownership...");
        invoke(
            &owner_change_ix,
            &[
                temp_token_account.clone(),
                initializer.clone(),
                token_program.clone(),
            ],
        )?;

        Ok(())
    }

    fn process_exchange(
        accounts: &[AccountInfo],
        amount_expected_by_taker: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let taker = next_account_info(account_info_iter)?;
        let takers_token_to_receive_account = next_account_info(account_info_iter)?;
        let pdas_temp_token_account = next_account_info(account_info_iter)?;
        let initializers_main_account = next_account_info(account_info_iter)?;
        let escrow_account = next_account_info(account_info_iter)?;
        let sales_tax_recipient = next_account_info(account_info_iter)?;

        let mint = next_account_info(account_info_iter)?;
        let metadata_account = next_account_info(account_info_iter)?;

        let token_program = next_account_info(account_info_iter)?;
        let system_program = next_account_info(account_info_iter)?;
        let pda_account = next_account_info(account_info_iter)?;

        let creator_accounts: Vec<AccountInfo> = account_info_iter.cloned().collect(); // rest 


        if !taker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }


        let pdas_temp_token_account_info =
            TokenAccount::unpack(&pdas_temp_token_account.data.borrow())?;
        let (pda, nonce) = Pubkey::find_program_address(&[ESCROW_PDA_SEED], program_id);

        if amount_expected_by_taker != pdas_temp_token_account_info.amount {
            return Err(EscrowError::ExpectedAmountMismatch.into());
        }
        msg!("amount: {:?}", amount_expected_by_taker);


        let escrow_info = Escrow::unpack(&escrow_account.data.borrow())?;

        if escrow_info.temp_token_account_pubkey != *pdas_temp_token_account.key {
            msg!("Deposit account not owned by this program's PDA");
            return Err(ProgramError::InvalidAccountData);
        }

        if escrow_info.initializer_pubkey != *initializers_main_account.key {
            msg!("Escrow data account not owned by the initializer");
            return Err(ProgramError::InvalidAccountData);
        }

        let str_pk: &[u8] = &bs58::decode(SALES_TAX_RECIPIENT_INTERNAL).into_vec().expect("BUG decoding sales tax recipient!")[..];
        if *sales_tax_recipient.key != Pubkey::new(str_pk) {
            msg!("Invalid sales tax recipient: {:?}", sales_tax_recipient);
            return Err(EscrowError::InvalidSalesTaxRecipient.into());
        }

        if escrow_info.mint_pubkey != *mint.key {
            msg!("Mint in escrow {:?} doesn't match passed mint {:?}", escrow_info.mint_pubkey, mint.key);
            return Err(ProgramError::InvalidAccountData);
        }

        let mda_derived = get_metadata_account(&mint.key);
        if mda_derived != *metadata_account.key {
            msg!("Mint-derived metadata account {:?} doesn't match passed metadata account {:?}", mda_derived, metadata_account.key);
            return Err(ProgramError::InvalidAccountData);
        }

        if *taker.key != escrow_info.initializer_pubkey {
            // Not a cancellation, so we need to process payment, sales tax and royalties.
            msg!("Transfering sales tax");
            let am = escrow_info.expected_amount;
            let tax_amount: u64 = (am * SALES_TAX)/10000;
            let xfer_sales_tax = system_instruction::transfer(&taker.key, &sales_tax_recipient.key, tax_amount);
            invoke(&xfer_sales_tax, &[taker.clone(), sales_tax_recipient.clone(), system_program.clone()])?;

            let mut royalty_total: u64 = 0;
            let res = Metadata::from_u8(&metadata_account.data.borrow());
            if res.is_ok() {
                let md = res.unwrap();
                royalty_total = (md.data.seller_fee_basis_points as u64 * am)/10000;
                if md.data.seller_fee_basis_points as u64 + SALES_TAX > 10000 {
                    return Err(EscrowError::InvalidRoyaltyFee.into());
                }

                // Note: we are disregarding the primary_sale_happened flag,
                // because a lot of collections/minters are not using it properly.
                msg!("Disbursing royalties...");

                match md.data.creators {
                    Some(creators) => {
                        if creators.len() != creator_accounts.len() {
                            msg!("number of creators in metadata {:?} doesn't match number of creators passed {:?}", creators.len(), creator_accounts.len());
                                return Err(EscrowError::CreatorMismatch.into());
                        }
                        let mut i = 0;
                        for creator in creators {
                            if creator.address != *creator_accounts[i].key {
                                msg!("creator {:?} in metadata {:?} doesn't match creator passed {:?}", i, creator.address, creator_accounts[i]);
                                return Err(EscrowError::CreatorMismatch.into());
                            }
                            let amount = (creator.share as u64 * royalty_total)/100;
                            let xfer = system_instruction::transfer(&taker.key, &creator.address, amount);
                            invoke(&xfer, &[taker.clone(), creator_accounts[i].clone(), system_program.clone()])?;
                            i += 1;
                        }
                    },
                    None => msg!("no creators => no payouts"),
                }
            } else {
                if let Err(e) = res {
                    // TODO discern between missing and invalid
                    msg!("no metadata found or metadata invalid, skipping royalties: {:?}", e);
                }
            }

            msg!("Transfering payment to initializer.");
            let final_amount_for_seller: u64 = am - tax_amount - royalty_total;
            if final_amount_for_seller <= 0 {
                msg!("Final amount {:?} is non-positive / tax={:?} / royalties={:?}", final_amount_for_seller, tax_amount, royalty_total);
                return Err(EscrowError::InvalidFinalAmount.into());
            }
            let xfer_lamports = system_instruction::transfer(&taker.key, &escrow_info.initializer_pubkey, final_amount_for_seller as u64);
            invoke(&xfer_lamports, &[taker.clone(), initializers_main_account.clone(), system_program.clone()])?;
        }

        msg!("Calling the token program to transfer tokens to the taker...");
        let transfer_to_taker_ix = spl_token::instruction::transfer(
            token_program.key,
            pdas_temp_token_account.key,
            takers_token_to_receive_account.key,
            &pda,
            &[&pda],
            pdas_temp_token_account_info.amount,
        )?;
        invoke_signed(
            &transfer_to_taker_ix,
            &[
                pdas_temp_token_account.clone(),
                takers_token_to_receive_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&ESCROW_PDA_SEED[..], &[nonce]]],
        )?;

        let close_pdas_temp_acc_ix = spl_token::instruction::close_account(
            token_program.key,
            pdas_temp_token_account.key,
            initializers_main_account.key,
            &pda,
            &[&pda],
        )?;

        msg!("Calling the token program to close pda's temp account...");
        invoke_signed(
            &close_pdas_temp_acc_ix,
            &[
                pdas_temp_token_account.clone(),
                initializers_main_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&ESCROW_PDA_SEED[..], &[nonce]]],
        )?;

        msg!("Closing the escrow account...");
        **initializers_main_account.lamports.borrow_mut() = initializers_main_account
            .lamports()
            .checked_add(escrow_account.lamports())
            .ok_or(EscrowError::AmountOverflow)?;
        **escrow_account.lamports.borrow_mut() = 0;

        Ok(())
    }
}
