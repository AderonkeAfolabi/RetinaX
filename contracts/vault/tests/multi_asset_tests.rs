#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

//! Multi-asset support and accounting for the vault contract (#337).

extern crate std;

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env, String, Vec,
};
use vault::{
    accounting::{MAX_LEDGER_PAGE, MAX_SUPPORTED_ASSETS},
    LedgerEntryKind, VaultContract, VaultContractClient, VaultError,
};

// ── Mock identity ───────────────────────────────────────────────────────────
//
// The real identity contract has a single active owner per deployment, so it
// can only back one configured vault. These tests need several vaults, so they
// use a mock exposing the one call the vault makes: `get_guardians`.

#[contract]
struct MockIdentity;

#[contractimpl]
impl MockIdentity {
    pub fn set_guardians(env: Env, owner: Address, guardians: Vec<Address>) {
        env.storage().persistent().set(&owner, &guardians);
    }

    pub fn get_guardians(env: Env, owner: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&owner)
            .unwrap_or_else(|| Vec::new(&env))
    }
}

struct Setup<'a> {
    env: Env,
    vault: VaultContractClient<'a>,
    vault_id: Address,
    identity: MockIdentityClient<'a>,
    admin: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let identity_id = env.register(MockIdentity, ());
    let identity = MockIdentityClient::new(&env, &identity_id);
    let vault_id = env.register(VaultContract, ());
    let vault = VaultContractClient::new(&env, &vault_id);
    let admin = Address::generate(&env);
    vault.initialize(&admin, &identity_id);

    Setup {
        env,
        vault,
        vault_id,
        identity,
        admin,
    }
}

impl<'a> Setup<'a> {
    /// Creates an owner with a configured 2-of-3 vault.
    fn new_vault(&self) -> Address {
        let owner = Address::generate(&self.env);
        let mut guardians = Vec::new(&self.env);
        for _ in 0..3 {
            guardians.push_back(Address::generate(&self.env));
        }
        self.identity.set_guardians(&owner, &guardians);
        self.vault.configure_vault(
            &owner,
            &2,
            &3,
            &2,
            &86_400,
            &String::from_str(&self.env, "ipfs://vault"),
            &BytesN::from_array(&self.env, &[9u8; 32]),
        );
        owner
    }

    /// Creates a Stellar asset contract and approves it in the vault.
    fn new_asset(&self) -> Address {
        let asset = self.unlisted_asset();
        self.vault.add_supported_asset(&self.admin, &asset);
        asset
    }

    fn unlisted_asset(&self) -> Address {
        let issuer = Address::generate(&self.env);
        self.env
            .register_stellar_asset_contract_v2(issuer)
            .address()
    }

    fn mint(&self, asset: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(&self.env, asset).mint(to, &amount);
    }

    fn token_balance(&self, asset: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, asset).balance(who)
    }

    /// Asserts the vault's accounting total equals the tokens it holds.
    fn assert_reconciled(&self, asset: &Address) {
        assert_eq!(
            self.vault.get_total_balance(asset),
            self.token_balance(asset, &self.vault_id),
            "accounting total diverged from custody"
        );
    }
}

// ── Supported assets ────────────────────────────────────────────────────────

#[test]
fn admin_manages_supported_assets() {
    let t = setup();
    let usdc = t.unlisted_asset();
    let eurc = t.unlisted_asset();
    assert!(t.vault.get_supported_assets().is_empty());
    assert!(!t.vault.is_supported_asset(&usdc));

    t.vault.add_supported_asset(&t.admin, &usdc);
    t.vault.add_supported_asset(&t.admin, &eurc);
    // Adding twice is a no-op.
    t.vault.add_supported_asset(&t.admin, &usdc);
    assert_eq!(t.vault.get_supported_assets().len(), 2);
    assert!(t.vault.is_supported_asset(&usdc));

    t.vault.remove_supported_asset(&t.admin, &usdc);
    assert!(!t.vault.is_supported_asset(&usdc));
    assert!(t.vault.is_supported_asset(&eurc));
    // Removing an absent asset is a no-op.
    t.vault.remove_supported_asset(&t.admin, &usdc);
    assert_eq!(t.vault.get_supported_assets().len(), 1);
}

#[test]
fn non_admin_cannot_manage_assets() {
    let t = setup();
    let asset = t.unlisted_asset();
    let intruder = Address::generate(&t.env);
    assert_eq!(
        t.vault.try_add_supported_asset(&intruder, &asset),
        Err(Ok(VaultError::Unauthorized))
    );
    t.vault.add_supported_asset(&t.admin, &asset);
    assert_eq!(
        t.vault.try_remove_supported_asset(&intruder, &asset),
        Err(Ok(VaultError::Unauthorized))
    );
}

#[test]
fn supported_asset_registry_is_bounded() {
    let t = setup();
    for _ in 0..MAX_SUPPORTED_ASSETS {
        t.new_asset();
    }
    let one_more = t.unlisted_asset();
    assert_eq!(
        t.vault.try_add_supported_asset(&t.admin, &one_more),
        Err(Ok(VaultError::AssetLimitReached))
    );
}

#[test]
fn asset_management_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let vault = VaultContractClient::new(&env, &env.register(VaultContract, ()));
    let someone = Address::generate(&env);
    assert_eq!(
        vault.try_add_supported_asset(&someone, &someone),
        Err(Ok(VaultError::NotInitialized))
    );
    assert_eq!(
        vault.try_deposit(&someone, &someone, &1),
        Err(Ok(VaultError::NotInitialized))
    );
}

// ── Deposits and per-asset balances ─────────────────────────────────────────

#[test]
fn deposits_track_each_asset_separately() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    let eurc = t.new_asset();
    let xlm = t.new_asset();
    t.mint(&usdc, &owner, 1_000);
    t.mint(&eurc, &owner, 500);
    t.mint(&xlm, &owner, 10_000);

    t.vault.deposit(&owner, &usdc, &400);
    t.vault.deposit(&owner, &eurc, &500);
    t.vault.deposit(&owner, &usdc, &100);
    t.vault.deposit(&owner, &xlm, &2_500);

    assert_eq!(t.vault.get_balance(&owner, &usdc), 500);
    assert_eq!(t.vault.get_balance(&owner, &eurc), 500);
    assert_eq!(t.vault.get_balance(&owner, &xlm), 2_500);

    // Tokens actually moved into custody.
    assert_eq!(t.token_balance(&usdc, &owner), 500);
    assert_eq!(t.token_balance(&usdc, &t.vault_id), 500);
    assert_eq!(t.token_balance(&eurc, &owner), 0);

    let balances = t.vault.get_balances(&owner);
    assert_eq!(balances.len(), 3);
    assert_eq!(balances.get(0).unwrap().asset, usdc);
    assert_eq!(balances.get(0).unwrap().balance, 500);
    assert_eq!(balances.get(1).unwrap().asset, eurc);
    assert_eq!(balances.get(2).unwrap().asset, xlm);

    for asset in [&usdc, &eurc, &xlm] {
        t.assert_reconciled(asset);
    }
}

#[test]
fn balances_are_isolated_between_vaults() {
    let t = setup();
    let alice = t.new_vault();
    let bob = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &alice, 300);
    t.mint(&usdc, &bob, 700);

    t.vault.deposit(&alice, &usdc, &300);
    t.vault.deposit(&bob, &usdc, &700);

    assert_eq!(t.vault.get_balance(&alice, &usdc), 300);
    assert_eq!(t.vault.get_balance(&bob, &usdc), 700);
    assert_eq!(t.vault.get_total_balance(&usdc), 1_000);
    t.assert_reconciled(&usdc);
}

#[test]
fn deposit_rejects_invalid_requests() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    let unlisted = t.unlisted_asset();
    t.mint(&usdc, &owner, 100);
    t.mint(&unlisted, &owner, 100);

    assert_eq!(
        t.vault.try_deposit(&owner, &unlisted, &10),
        Err(Ok(VaultError::UnsupportedAsset))
    );
    assert_eq!(
        t.vault.try_deposit(&owner, &usdc, &0),
        Err(Ok(VaultError::InvalidAmount))
    );
    assert_eq!(
        t.vault.try_deposit(&owner, &usdc, &-5),
        Err(Ok(VaultError::InvalidAmount))
    );

    // An address without a configured vault cannot deposit.
    let stranger = Address::generate(&t.env);
    t.mint(&usdc, &stranger, 100);
    assert_eq!(
        t.vault.try_deposit(&stranger, &usdc, &10),
        Err(Ok(VaultError::VaultNotFound))
    );

    // Nothing was recorded by the rejected calls.
    assert_eq!(t.vault.get_ledger_entry_count(&owner), 0);
    assert_eq!(t.vault.get_total_balance(&usdc), 0);
}

#[test]
fn deposit_beyond_token_balance_fails_atomically() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &owner, 50);

    assert!(t.vault.try_deposit(&owner, &usdc, &51).is_err());
    assert_eq!(t.vault.get_balance(&owner, &usdc), 0);
    assert_eq!(t.vault.get_ledger_entry_count(&owner), 0);
    assert_eq!(t.token_balance(&usdc, &owner), 50);
}

#[test]
fn deposit_requires_owner_auth() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &owner, 100);

    t.vault.deposit(&owner, &usdc, &10);
    let auths = t.env.auths();
    assert!(auths.iter().any(|(addr, _)| *addr == owner));
}

// ── Withdrawals ─────────────────────────────────────────────────────────────

#[test]
fn withdraw_sends_tokens_and_updates_balance() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    let recipient = Address::generate(&t.env);
    t.mint(&usdc, &owner, 1_000);
    t.vault.deposit(&owner, &usdc, &1_000);

    let entry = t.vault.withdraw(&owner, &usdc, &300, &recipient);
    assert_eq!(entry.kind, LedgerEntryKind::Withdrawal);
    assert_eq!(entry.amount, 300);
    assert_eq!(entry.balance_after, 700);
    assert_eq!(entry.counterparty, recipient);

    assert_eq!(t.vault.get_balance(&owner, &usdc), 700);
    assert_eq!(t.token_balance(&usdc, &recipient), 300);
    t.assert_reconciled(&usdc);
}

#[test]
fn withdraw_rejects_overdraft_and_bad_amounts() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    let eurc = t.new_asset();
    t.mint(&usdc, &owner, 100);
    t.vault.deposit(&owner, &usdc, &100);

    assert_eq!(
        t.vault.try_withdraw(&owner, &usdc, &101, &owner),
        Err(Ok(VaultError::InsufficientBalance))
    );
    // Holding USDC does not let the owner withdraw a different asset.
    assert_eq!(
        t.vault.try_withdraw(&owner, &eurc, &1, &owner),
        Err(Ok(VaultError::InsufficientBalance))
    );
    assert_eq!(
        t.vault.try_withdraw(&owner, &usdc, &0, &owner),
        Err(Ok(VaultError::InvalidAmount))
    );
    assert_eq!(t.vault.get_balance(&owner, &usdc), 100);
}

#[test]
fn full_withdrawal_drops_asset_from_balance_list() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    let eurc = t.new_asset();
    t.mint(&usdc, &owner, 100);
    t.mint(&eurc, &owner, 100);
    t.vault.deposit(&owner, &usdc, &100);
    t.vault.deposit(&owner, &eurc, &100);

    t.vault.withdraw(&owner, &usdc, &100, &owner);
    let balances = t.vault.get_balances(&owner);
    assert_eq!(balances.len(), 1);
    assert_eq!(balances.get(0).unwrap().asset, eurc);
    assert_eq!(t.vault.get_balance(&owner, &usdc), 0);
}

#[test]
fn delisted_asset_can_still_be_withdrawn_but_not_deposited() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &owner, 200);
    t.vault.deposit(&owner, &usdc, &100);

    t.vault.remove_supported_asset(&t.admin, &usdc);
    assert_eq!(
        t.vault.try_deposit(&owner, &usdc, &50),
        Err(Ok(VaultError::UnsupportedAsset))
    );
    t.vault.withdraw(&owner, &usdc, &100, &owner);
    assert_eq!(t.token_balance(&usdc, &owner), 200);
    t.assert_reconciled(&usdc);
}

// ── Transfers between vaults ────────────────────────────────────────────────

#[test]
fn transfer_moves_balance_without_moving_tokens() {
    let t = setup();
    let alice = t.new_vault();
    let bob = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &alice, 1_000);
    t.vault.deposit(&alice, &usdc, &1_000);
    let custody_before = t.token_balance(&usdc, &t.vault_id);

    let receipt = t.vault.transfer_between_vaults(&alice, &bob, &usdc, &250);
    assert_eq!(receipt.amount, 250);
    assert_eq!(receipt.from_balance_after, 750);
    assert_eq!(receipt.to_balance_after, 250);
    assert_eq!(receipt.from_seq, 2); // deposit was #1
    assert_eq!(receipt.to_seq, 1);

    assert_eq!(t.vault.get_balance(&alice, &usdc), 750);
    assert_eq!(t.vault.get_balance(&bob, &usdc), 250);
    // Custody and the accounting total are unchanged by an internal transfer.
    assert_eq!(t.token_balance(&usdc, &t.vault_id), custody_before);
    assert_eq!(t.vault.get_total_balance(&usdc), 1_000);
    t.assert_reconciled(&usdc);

    // Bob can withdraw what he received.
    t.vault.withdraw(&bob, &usdc, &250, &bob);
    assert_eq!(t.token_balance(&usdc, &bob), 250);
    t.assert_reconciled(&usdc);
}

#[test]
fn transfers_of_different_assets_are_independent() {
    let t = setup();
    let alice = t.new_vault();
    let bob = t.new_vault();
    let usdc = t.new_asset();
    let eurc = t.new_asset();
    t.mint(&usdc, &alice, 100);
    t.mint(&eurc, &bob, 100);
    t.vault.deposit(&alice, &usdc, &100);
    t.vault.deposit(&bob, &eurc, &100);

    t.vault.transfer_between_vaults(&alice, &bob, &usdc, &40);
    t.vault.transfer_between_vaults(&bob, &alice, &eurc, &60);

    assert_eq!(t.vault.get_balance(&alice, &usdc), 60);
    assert_eq!(t.vault.get_balance(&alice, &eurc), 60);
    assert_eq!(t.vault.get_balance(&bob, &usdc), 40);
    assert_eq!(t.vault.get_balance(&bob, &eurc), 40);
    assert_eq!(t.vault.get_balances(&alice).len(), 2);
    assert_eq!(t.vault.get_balances(&bob).len(), 2);
}

#[test]
fn transfer_rejects_invalid_requests() {
    let t = setup();
    let alice = t.new_vault();
    let bob = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &alice, 100);
    t.vault.deposit(&alice, &usdc, &100);
    let no_vault = Address::generate(&t.env);

    assert_eq!(
        t.vault
            .try_transfer_between_vaults(&alice, &alice, &usdc, &10),
        Err(Ok(VaultError::SelfTransfer))
    );
    assert_eq!(
        t.vault.try_transfer_between_vaults(&alice, &bob, &usdc, &0),
        Err(Ok(VaultError::InvalidAmount))
    );
    assert_eq!(
        t.vault
            .try_transfer_between_vaults(&alice, &bob, &usdc, &101),
        Err(Ok(VaultError::InsufficientBalance))
    );
    assert_eq!(
        t.vault
            .try_transfer_between_vaults(&alice, &no_vault, &usdc, &10),
        Err(Ok(VaultError::VaultNotFound))
    );
    assert_eq!(
        t.vault
            .try_transfer_between_vaults(&no_vault, &alice, &usdc, &10),
        Err(Ok(VaultError::VaultNotFound))
    );

    t.vault.remove_supported_asset(&t.admin, &usdc);
    assert_eq!(
        t.vault
            .try_transfer_between_vaults(&alice, &bob, &usdc, &10),
        Err(Ok(VaultError::UnsupportedAsset))
    );

    // No rejected call touched either trail.
    assert_eq!(t.vault.get_ledger_entry_count(&alice), 1);
    assert_eq!(t.vault.get_ledger_entry_count(&bob), 0);
    assert_eq!(t.vault.get_balance(&alice, &usdc), 100);
}

// ── Accounting audit trail ──────────────────────────────────────────────────

#[test]
fn audit_trail_records_every_movement_in_order() {
    let t = setup();
    let alice = t.new_vault();
    let bob = t.new_vault();
    let usdc = t.new_asset();
    let eurc = t.new_asset();
    let payee = Address::generate(&t.env);
    t.mint(&usdc, &alice, 1_000);
    t.mint(&eurc, &alice, 1_000);

    t.env.ledger().set_timestamp(1_700_000_100);
    t.vault.deposit(&alice, &usdc, &500);
    t.env.ledger().set_timestamp(1_700_000_200);
    t.vault.deposit(&alice, &eurc, &300);
    t.env.ledger().set_timestamp(1_700_000_300);
    t.vault.transfer_between_vaults(&alice, &bob, &usdc, &200);
    t.env.ledger().set_timestamp(1_700_000_400);
    t.vault.withdraw(&alice, &usdc, &100, &payee);

    assert_eq!(t.vault.get_ledger_entry_count(&alice), 4);
    let trail = t.vault.get_ledger_entries(&alice, &1, &10);
    assert_eq!(trail.len(), 4);

    let expected = [
        (
            LedgerEntryKind::Deposit,
            &usdc,
            500,
            &alice,
            500,
            1_700_000_100u64,
        ),
        (
            LedgerEntryKind::Deposit,
            &eurc,
            300,
            &alice,
            300,
            1_700_000_200,
        ),
        (
            LedgerEntryKind::TransferOut,
            &usdc,
            200,
            &bob,
            300,
            1_700_000_300,
        ),
        (
            LedgerEntryKind::Withdrawal,
            &usdc,
            100,
            &payee,
            200,
            1_700_000_400,
        ),
    ];
    for (i, (kind, asset, amount, counterparty, after, ts)) in expected.iter().enumerate() {
        let e = trail.get(i as u32).unwrap();
        assert_eq!(e.seq, i as u64 + 1);
        assert_eq!(e.kind, *kind);
        assert_eq!(&e.asset, *asset);
        assert_eq!(e.amount, *amount);
        assert_eq!(&e.counterparty, *counterparty);
        assert_eq!(e.balance_after, *after);
        assert_eq!(e.timestamp, *ts);
    }

    // The recipient side of the transfer is mirrored in Bob's trail.
    let bob_trail = t.vault.get_ledger_entries(&bob, &1, &10);
    assert_eq!(bob_trail.len(), 1);
    let incoming = bob_trail.get(0).unwrap();
    assert_eq!(incoming.kind, LedgerEntryKind::TransferIn);
    assert_eq!(incoming.counterparty, alice);
    assert_eq!(incoming.amount, 200);
    assert_eq!(incoming.balance_after, 200);
}

#[test]
fn audit_trail_replays_to_current_balances() {
    let t = setup();
    let alice = t.new_vault();
    let bob = t.new_vault();
    let assets = [t.new_asset(), t.new_asset(), t.new_asset()];
    for a in assets.iter() {
        t.mint(a, &alice, 10_000);
    }

    // A deterministic mix of operations across three assets.
    for round in 0..12i128 {
        let a = &assets[(round % 3) as usize];
        t.vault.deposit(&alice, a, &(100 + round * 7));
        if round % 2 == 0 {
            t.vault
                .transfer_between_vaults(&alice, &bob, a, &(10 + round));
        }
        if round % 4 == 3 {
            t.vault.withdraw(&alice, a, &25, &alice);
        }
    }

    // Replaying the trail reproduces each per-asset balance exactly.
    let count = t.vault.get_ledger_entry_count(&alice);
    let mut replayed = std::collections::BTreeMap::<u32, i128>::new();
    let mut seq = 1u64;
    while seq <= count {
        let page = t.vault.get_ledger_entries(&alice, &seq, &MAX_LEDGER_PAGE);
        assert!(!page.is_empty());
        for e in page.iter() {
            let idx = assets.iter().position(|a| *a == e.asset).unwrap() as u32;
            let bal = replayed.entry(idx).or_insert(0);
            match e.kind {
                LedgerEntryKind::Deposit | LedgerEntryKind::TransferIn => *bal += e.amount,
                LedgerEntryKind::Withdrawal | LedgerEntryKind::TransferOut => *bal -= e.amount,
            }
            assert_eq!(
                *bal, e.balance_after,
                "entry {} balance_after mismatch",
                e.seq
            );
            seq = e.seq + 1;
        }
    }
    for (i, a) in assets.iter().enumerate() {
        assert_eq!(replayed[&(i as u32)], t.vault.get_balance(&alice, a));
        t.assert_reconciled(a);
    }
}

#[test]
fn audit_trail_pagination() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    let total = MAX_LEDGER_PAGE + 5;
    t.mint(&usdc, &owner, total as i128);
    for _ in 0..total {
        t.vault.deposit(&owner, &usdc, &1);
    }
    assert_eq!(t.vault.get_ledger_entry_count(&owner), total as u64);

    // Page size is capped.
    let first = t.vault.get_ledger_entries(&owner, &1, &1_000);
    assert_eq!(first.len(), MAX_LEDGER_PAGE);
    assert_eq!(first.get(0).unwrap().seq, 1);

    let rest = t
        .vault
        .get_ledger_entries(&owner, &(MAX_LEDGER_PAGE as u64 + 1), &MAX_LEDGER_PAGE);
    assert_eq!(rest.len(), 5);
    assert_eq!(rest.get(4).unwrap().seq, total as u64);

    // Start 0 is treated as 1; past-the-end and zero-limit pages are empty.
    assert_eq!(
        t.vault
            .get_ledger_entries(&owner, &0, &1)
            .get(0)
            .unwrap()
            .seq,
        1
    );
    assert!(t
        .vault
        .get_ledger_entries(&owner, &(total as u64 + 1), &5)
        .is_empty());
    assert!(t.vault.get_ledger_entries(&owner, &1, &0).is_empty());

    assert_eq!(t.vault.get_ledger_entry(&owner, &3).unwrap().seq, 3);
    assert!(t
        .vault
        .get_ledger_entry(&owner, &(total as u64 + 1))
        .is_none());
}

#[test]
fn accounting_emits_events() {
    let t = setup();
    let owner = t.new_vault();
    let usdc = t.new_asset();
    t.mint(&usdc, &owner, 100);

    t.vault.deposit(&owner, &usdc, &100);
    // Filter to the vault's own events (the token also emits a transfer event).
    let vault_events = t.env.events().all().filter_by_contract(&t.vault_id);
    assert_eq!(vault_events.events().len(), 1);
}
