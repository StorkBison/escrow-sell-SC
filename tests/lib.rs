use solana_escrow::state::Escrow;
use solana_escrow::*;
use solana_program::program_error::ProgramError;
use solana_program::program_pack::Pack;
use solana_program_test::*;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction, InstructionError},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    sysvar::{rent::Rent, Sysvar},
    transaction::{Transaction, TransactionError},
};
use std::borrow::BorrowMut;
use std::str::FromStr;

mod utils;

#[tokio::test]
async fn test_processor() {
    let program_id = Pubkey::new_unique();
    let _ = processor::Processor::process(&program_id, &[], &[0, 5, 0]);
}

#[tokio::test]
async fn test_entrypoint_unsafe() {
    let mut input = vec![
        1u8,
        0u8,
        0u8,
        0u8,
        0u8,
        0u8,
        0u8,
        0u8,
        u8::MAX,
        0u8,
        0u8,
        0u8,
        0u8,
        0u8,
        0u8,
        0u8,
    ];
    let mut other_bytes = vec![0u8; 32 * 2 + 8 * 2 + 1024 * 10];
    input.append(&mut other_bytes);
    unsafe {
        entrypoint::entrypoint(input.as_mut_ptr());
    }
}

#[tokio::test]
async fn test_invalid_account_data() {
    let escrow_data = vec![2_u8; state::Escrow::LEN];
    let res = Escrow::unpack_from_slice(&escrow_data);
    assert_eq!(res.err().unwrap(), ProgramError::InvalidAccountData);
}

#[tokio::test]
async fn test_init_with_not_enough_accounts_should_fail() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();

    let program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::NotEnoughAccountKeys)
    );
}

#[tokio::test]
async fn test_init_when_escrow_account_not_rent_exempt() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 500,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::Custom(1))
    );
}

#[tokio::test]
async fn test_init_when_initializer_not_signed_should_fail() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    let accounts = vec![
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::MissingRequiredSignature)
    );
}

#[tokio::test]
async fn test_init_when_mint_not_created_should_fail() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidAccountData)
    );
}

#[tokio::test]
async fn test_init_when_token_not_created_should_fail() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidAccountData)
    );
}

#[tokio::test]
async fn test_init_with_invalid_mint_should_fail() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let wrong_mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_mint(
        &mut program_ctx,
        &wrong_mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(wrong_mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::Custom(6))
    );
}

#[tokio::test]
async fn test_init_with_invalid_rent() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let wrong_rent_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    program_test.add_account(
        wrong_rent_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; Rent::size_of()],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(wrong_rent_keypair.pubkey(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidArgument)
    );
}

#[tokio::test]
async fn test_init_initialized_escrow() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![1_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );

    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::AccountAlreadyInitialized)
    );
}

#[tokio::test]
async fn test_init_escrow_with_wrong_amount() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );

    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        2,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::Custom(7))
    );
}

#[tokio::test]
async fn test_invalid_instruction() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );

    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        2,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[2, 5, 0],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::Custom(0))
    );
}

#[tokio::test]
async fn test_init_escrow() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    program_ctx
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let escrow_account = program_ctx
        .banks_client
        .get_account(escrow_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let escrow = state::Escrow::unpack_from_slice(&escrow_account.data).unwrap();
    assert_eq!(escrow.is_initialized, true);
    assert_eq!(escrow.initializer_pubkey, payer_keypair.pubkey());
    assert_eq!(
        escrow.temp_token_account_pubkey,
        token_account_keypair.pubkey()
    );
    assert_eq!(escrow.expected_amount, 1);

    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    let token_account = program_ctx
        .banks_client
        .get_account(token_account_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token = spl_token::state::Account::unpack_from_slice(&token_account.data).unwrap();
    assert_eq!(token.owner, pda);

    let sales_tax_account = program_ctx
        .banks_client
        .get_account(sales_tax_recipient_pubkey)
        .await
        .expect("get_account")
        .expect("account not found");
    assert_eq!(sales_tax_account.lamports, utils::LISTING_FEE);
}

#[tokio::test]
async fn test_init_several_escrows() {
    let program_id = Pubkey::new_unique();
    let mint_keypair = Keypair::new();
    let escrow_1_keypair = Keypair::new();
    let escrow_2_keypair = Keypair::new();
    let token_1_account_keypair = Keypair::new();
    let token_2_account_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    program_test.add_account(
        escrow_1_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_2_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![0_u8; state::Escrow::LEN],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_1_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_1_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &token_2_account_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_2_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let mut accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_1_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_1_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    program_ctx
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(token_2_account_keypair.pubkey(), false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(escrow_2_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(Pubkey::from_str(utils::RENT_ACCOUNT_STR).unwrap(), false),
        AccountMeta::new(spl_token::id(), false),
    ];
    transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[0u8, 255u8, 255u8, 255u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    program_ctx
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    let escrow1_account = program_ctx
        .banks_client
        .get_account(escrow_1_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let escrow2_account = program_ctx
        .banks_client
        .get_account(escrow_2_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let escrow1 = state::Escrow::unpack_from_slice(&escrow1_account.data).unwrap();
    let escrow2 = state::Escrow::unpack_from_slice(&escrow2_account.data).unwrap();
    assert_eq!(escrow1.is_initialized, true);
    assert_eq!(escrow1.initializer_pubkey, payer_keypair.pubkey());
    assert_eq!(
        escrow1.temp_token_account_pubkey,
        token_1_account_keypair.pubkey()
    );
    assert_eq!(escrow1.expected_amount, 1);
    assert_eq!(escrow2.is_initialized, true);
    assert_eq!(escrow2.initializer_pubkey, payer_keypair.pubkey());
    assert_eq!(
        escrow2.temp_token_account_pubkey,
        token_2_account_keypair.pubkey()
    );
    assert_eq!(escrow2.expected_amount, 16777215);

    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    let token1_account = program_ctx
        .banks_client
        .get_account(token_1_account_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token2_account = program_ctx
        .banks_client
        .get_account(token_2_account_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token1 = spl_token::state::Account::unpack_from_slice(&token1_account.data).unwrap();
    let token2 = spl_token::state::Account::unpack_from_slice(&token2_account.data).unwrap();
    assert_eq!(token1.owner, pda);
    assert_eq!(token2.owner, pda);
}

// PROCESS ESCROW TESTS TODO:
#[tokio::test]
async fn test_process_escrow() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();
    let sales_tax_recipient_pubkey = Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap();
    let initial_sales_tax_recipient_lamports = 1u64;
    let price: u64 = 1000;

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: token_account_keypair.pubkey(),
        expected_amount: price,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    program_test.add_account(
        sales_tax_recipient_pubkey,
        Account {
            lamports: initial_sales_tax_recipient_lamports,
            data: vec![],
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    let escrow_account = program_ctx
        .banks_client
        .get_account(escrow_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    assert_eq!(
        state::Escrow::unpack_from_slice(&escrow_account.data)
            .unwrap()
            .is_initialized,
        true
    );

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        price,
    )
    .await
    .unwrap();

    let initializer_account_before = program_ctx
        .banks_client
        .get_account(initializer_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let escrow_account_before = program_ctx
        .banks_client
        .get_account(initializer_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token_account_before = program_ctx
        .banks_client
        .get_account(token_account_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(sales_tax_recipient_pubkey, false),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            // 1 is the instruction code, [232u8, 3u8, ...] equals to 1000 little-endian u64
            &[1u8, 232u8, 3u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    program_ctx
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // Check that tokens are transferred
    let taker_token_account = program_ctx
        .banks_client
        .get_account(taker_token_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token = spl_token::state::Account::unpack_from_slice(&taker_token_account.data).unwrap();
    assert_eq!(token.amount, price);

    let tax_amount: f64 = price as f64 * utils::SALES_TAX;
    let initializer_account = program_ctx
        .banks_client
        .get_account(initializer_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    assert_eq!(initializer_account.lamports, initializer_account_before.lamports + escrow_account_before.lamports + token_account_before.lamports + price - tax_amount as u64);
    let sales_tax_account = program_ctx
        .banks_client
        .get_account(sales_tax_recipient_pubkey)
        .await
        .expect("get_account")
        .expect("account not found");
    assert_eq!(sales_tax_account.lamports, initial_sales_tax_recipient_lamports + tax_amount as u64);
}

#[tokio::test]
async fn test_cancel_escrow() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: token_account_keypair.pubkey(),
        expected_amount: 1,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    let escrow_account = program_ctx
        .banks_client
        .get_account(escrow_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    assert_eq!(
        state::Escrow::unpack_from_slice(&escrow_account.data)
            .unwrap()
            .is_initialized,
        true
    );

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let initializer_account_before = program_ctx
        .banks_client
        .get_account(initializer_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let escrow_account_before = program_ctx
        .banks_client
        .get_account(initializer_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token_account_before = program_ctx
        .banks_client
        .get_account(token_account_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");

    let accounts = vec![
        AccountMeta::new(initializer_keypair.pubkey(), true),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(
            Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap(),
            false,
        ),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[1u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(
        &[&payer_keypair, &initializer_keypair],
        program_ctx.last_blockhash,
    );
    program_ctx
        .banks_client
        .process_transaction(transaction)
        .await
        .unwrap();

    // Check that tokens are transferred
    let taker_token_account = program_ctx
        .banks_client
        .get_account(taker_token_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    let token = spl_token::state::Account::unpack_from_slice(&taker_token_account.data).unwrap();
    assert_eq!(token.amount, 1);

    let initializer_account = program_ctx
        .banks_client
        .get_account(initializer_keypair.pubkey())
        .await
        .expect("get_account")
        .expect("account not found");
    assert_eq!(initializer_account.lamports, initializer_account_before.lamports + escrow_account_before.lamports + token_account_before.lamports);
}

#[tokio::test]
async fn test_process_escrow_token_account_mismatch() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: initializer_keypair.pubkey(),
        expected_amount: 1,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(
            Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap(),
            false,
        ),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[1u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidAccountData)
    );
}

#[tokio::test]
async fn test_process_escrow_initializer_account_mismatch() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: token_account_keypair.pubkey(),
        expected_amount: 1,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(
            Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap(),
            false,
        ),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[1u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidAccountData)
    );
}

#[tokio::test]
async fn test_process_escrow_sales_tax_account_mismatch() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: token_account_keypair.pubkey(),
        expected_amount: 1,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(
            Pubkey::from_str("8Ba7LXjBTWScPKMV4Lmz5dsenz52NVAwJsKYXyf7TzFZ").unwrap(),
            false,
        ),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[1u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::Custom(4))
    );
}

#[tokio::test]
async fn test_process_escrow_expected_amount_mismatch() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: token_account_keypair.pubkey(),
        expected_amount: 1,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(payer_keypair.pubkey(), true),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(
            Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap(),
            false,
        ),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[1u8, 2u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::Custom(2))
    );
}

#[tokio::test]
async fn test_process_escrow_missing_signature() {
    let program_id = Pubkey::new_unique();
    let initializer_keypair = Keypair::new();
    let taker_keypair = Keypair::new();
    let mint_keypair = Keypair::new();
    let escrow_keypair = Keypair::new();
    let token_account_keypair = Keypair::new();
    let taker_token_keypair = Keypair::new();

    let mut program_test = ProgramTest::new(
        "escrow", // Run the BPF version with `cargo test-bpf`
        program_id,
        processor!(processor::Processor::process), // Run the native version with `cargo test`
    );
    let mut escrow_data = vec![0_u8; state::Escrow::LEN];
    let escrow_info = Escrow {
        is_initialized: true,
        initializer_pubkey: initializer_keypair.pubkey(),
        mint_pubkey: mint_keypair.pubkey(),
        temp_token_account_pubkey: token_account_keypair.pubkey(),
        expected_amount: 1,
    };
    Escrow::pack(escrow_info, &mut escrow_data.borrow_mut()).unwrap();
    program_test.add_account(
        escrow_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: escrow_data,
            owner: program_id,
            ..Account::default()
        },
    );
    program_test.add_account(
        initializer_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    program_test.add_account(
        taker_keypair.pubkey(),
        Account {
            lamports: 5000000,
            data: vec![],
            owner: solana_program::system_program::id(),
            ..Account::default()
        },
    );
    let mut program_ctx = program_test.start_with_context().await;
    let payer_keypair = Keypair::from_base58_string(&program_ctx.payer.to_base58_string());

    utils::create_mint(
        &mut program_ctx,
        &mint_keypair,
        100000000,
        &payer_keypair.pubkey(),
    )
    .await
    .unwrap();
    let (pda, _nonce) = Pubkey::find_program_address(&[b"escrow"], &program_id);
    utils::create_token_account(
        &mut program_ctx,
        &token_account_keypair,
        &mint_keypair.pubkey(),
        &pda,
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::create_token_account(
        &mut program_ctx,
        &taker_token_keypair,
        &mint_keypair.pubkey(),
        &payer_keypair.pubkey(),
        &Rent::default(),
    )
    .await
    .unwrap();
    utils::mint_tokens_to(
        &mut program_ctx,
        &mint_keypair.pubkey(),
        &token_account_keypair.pubkey(),
        &payer_keypair,
        1,
    )
    .await
    .unwrap();

    let accounts = vec![
        AccountMeta::new(taker_keypair.pubkey(), false),
        AccountMeta::new(taker_token_keypair.pubkey(), false),
        AccountMeta::new(token_account_keypair.pubkey(), false),
        AccountMeta::new(initializer_keypair.pubkey(), false),
        AccountMeta::new(escrow_keypair.pubkey(), false),
        AccountMeta::new(
            Pubkey::from_str(utils::SALES_TAX_ACCOUNT_STR).unwrap(),
            false,
        ),
        AccountMeta::new(mint_keypair.pubkey(), false),
        AccountMeta::new(utils::get_metadata_account(&mint_keypair.pubkey()), false),
        AccountMeta::new(spl_token::id(), false),
        AccountMeta::new(solana_program::system_program::id(), false),
        AccountMeta::new(pda, false),
    ];
    let mut transaction = Transaction::new_with_payer(
        &[Instruction::new_with_bincode(
            program_id,
            &[1u8, 1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8],
            accounts,
        )],
        Some(&payer_keypair.pubkey()),
    );
    transaction.sign(&[&payer_keypair], program_ctx.last_blockhash);
    assert_eq!(
        program_ctx
            .banks_client
            .process_transaction(transaction)
            .await
            .err()
            .unwrap()
            .unwrap(),
        TransactionError::InstructionError(0, InstructionError::MissingRequiredSignature)
    );
}
