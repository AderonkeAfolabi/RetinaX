#![no_std]
#![allow(clippy::too_many_arguments)]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, IntoVal, String,
    Symbol, Vec,
};

// ── Storage keys ────────────────────────────────────────────────────────────────

const CONFIG: Symbol = symbol_short!("CONFIG");
const PROPOSAL_CTR: Symbol = symbol_short!("PR_CTR");
const PROPOSAL: Symbol = symbol_short!("PROPOSAL");
const ALLOCATION: Symbol = symbol_short!("ALLOC");
const GOVERNOR: Symbol = symbol_short!("GOVERNOR");
const ASSET_LIMIT: Symbol = symbol_short!("LIMIT");
const CATEGORY_LIMIT: Symbol = symbol_short!("CAT_LIM");
const CATEGORY_SPENT: Symbol = symbol_short!("CAT_SPT");
const PERIOD_DURATION: Symbol = symbol_short!("PERIOD");

// ── Types ──────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryConfig {
    pub admin: Address,
    pub signers: Vec<Address>,
    pub threshold: u32,
    pub dex_router: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Executed,
    Expired,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferProposal {
    pub asset: Address,
    pub to: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwapProposal {
    pub path: Vec<Address>,
    pub amount_in: i128,
    pub min_out: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalKind {
    Transfer(TransferProposal),
    Swap(SwapProposal),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub kind: ProposalKind,
    pub category: Symbol,
    pub description: String,
    pub approvals: Vec<Address>,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationSummary {
    pub category: Symbol,
    pub total_spent: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    pub id: u64,
    pub sender: Address,
    pub recipient: Address,
    pub asset: Address,
    pub deposit: i128,
    pub rate: i128,
    pub start_time: u64,
    pub stop_time: u64,
    pub cliff: u64,
    pub claimed_amount: i128,
    pub canceled: bool,
}

#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NoSigners = 3,
    InvalidThreshold = 4,
    PositiveAmountRequired = 5,
    UnauthorisedProposer = 6,
    FutureExpiryRequired = 7,
    UnauthorisedSigner = 8,
    ProposalNotFound = 9,
    ProposalNotPending = 10,
    ProposalExpired = 11,
    InsufficientApprovals = 12,
    NotAuthorizedCaller = 13,
    UnauthorisedAdmin = 14,
    LimitExceeded = 15,
    InvalidPath = 16,
    DexRouterNotConfigured = 17,
    SwapFailed = 18,
    StreamNotFound = 19,
    StreamAlreadyCanceled = 20,
    CliffNotReached = 21,
    NoClaimableTokens = 22,
    InvalidStreamSchedule = 23,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn is_signer(env: &Env, who: &Address) -> Result<bool, ContractError> {
    let cfg: TreasuryConfig = env
        .storage()
        .instance()
        .get(&CONFIG)
        .ok_or(ContractError::NotInitialized)?;
    Ok(cfg.signers.iter().any(|s| s == *who))
}

fn load_config(env: &Env) -> Result<TreasuryConfig, ContractError> {
    env.storage()
        .instance()
        .get(&CONFIG)
        .ok_or(ContractError::NotInitialized)
}

fn next_proposal_id(env: &Env) -> u64 {
    let current: u64 = env.storage().instance().get(&PROPOSAL_CTR).unwrap_or(0);
    let next = current.saturating_add(1);
    env.storage().instance().set(&PROPOSAL_CTR, &next);
    next
}

fn next_stream_id(env: &Env) -> u64 {
    let current: u64 = env.storage().instance().get(&STREAM_CTR).unwrap_or(0);
    let next = current.saturating_add(1);
    env.storage().instance().set(&STREAM_CTR, &next);
    next
}

fn stream_key(id: u64) -> (Symbol, u64) {
    (STREAM, id)
}

fn proposal_key(id: u64) -> (Symbol, u64) {
    (PROPOSAL, id)
}

fn allocation_key(asset: &Address, category: &Symbol) -> (Symbol, Address, Symbol) {
    (ALLOCATION, asset.clone(), category.clone())
}

fn asset_limit_key(asset: &Address) -> (Symbol, Address) {
    (ASSET_LIMIT, asset.clone())
}

fn get_asset_limit(env: &Env, asset: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&asset_limit_key(asset))
        .unwrap_or(i128::MAX)
}

fn category_limit_key(category: &Symbol) -> (Symbol, Symbol) {
    (CATEGORY_LIMIT, category.clone())
}

fn get_category_limit(env: &Env, category: &Symbol) -> i128 {
    env.storage()
        .instance()
        .get(&category_limit_key(category))
        .unwrap_or(i128::MAX)
}

fn category_spent_key(category: &Symbol, period_start: u64) -> (Symbol, Symbol, u64) {
    (CATEGORY_SPENT, category.clone(), period_start)
}

fn get_period_start(env: &Env, now: u64) -> u64 {
    let period: u64 = env
        .storage()
        .instance()
        .get(&PERIOD_DURATION)
        .unwrap_or(86400);
    (now / period) * period
}

fn get_category_spent(env: &Env, category: &Symbol, period_start: u64) -> i128 {
    env.storage()
        .instance()
        .get(&category_spent_key(category, period_start))
        .unwrap_or(0)
}

fn has_approval(_env: &Env, proposal: &Proposal, signer: &Address) -> bool {
    proposal.approvals.iter().any(|s| s == *signer)
}

fn count_approvals(proposal: &Proposal) -> u32 {
    proposal.approvals.len()
}

// ── Contract ───────────────────────────────────────────────────────────────────

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&CONFIG) {
            return Err(ContractError::AlreadyInitialized);
        }
        if signers.is_empty() {
            return Err(ContractError::NoSigners);
        }
        if threshold == 0 || threshold > signers.len() {
            return Err(ContractError::InvalidThreshold);
        }

        let cfg = TreasuryConfig {
            admin,
            signers,
            threshold,
            dex_router: None,
        };

        env.storage().instance().set(&CONFIG, &cfg);
        Ok(())
    }

    pub fn set_dex_router(env: Env, admin: Address, router: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let mut cfg = load_config(&env)?;
        if cfg.admin != admin {
            return Err(ContractError::UnauthorisedAdmin);
        }
        cfg.dex_router = Some(router);
        env.storage().instance().set(&CONFIG, &cfg);
        Ok(())
    }

    pub fn set_asset_limit(
        env: Env,
        admin: Address,
        asset: Address,
        limit: i128,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let cfg = load_config(&env)?;
        if cfg.admin != admin {
            return Err(ContractError::UnauthorisedAdmin);
        }
        env.storage()
            .instance()
            .set(&asset_limit_key(&asset), &limit);

        env.events().publish((symbol_short!("limit"), asset), limit);
        Ok(())
    }

    pub fn get_config(env: Env) -> Result<TreasuryConfig, ContractError> {
        load_config(&env)
    }

    pub fn get_limit(env: Env, asset: Address) -> i128 {
        get_asset_limit(&env, &asset)
    }

    pub fn set_category_limit(
        env: Env,
        admin: Address,
        category: Symbol,
        limit: i128,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let cfg = load_config(&env)?;
        if cfg.admin != admin {
            return Err(ContractError::UnauthorisedAdmin);
        }
        env.storage()
            .instance()
            .set(&category_limit_key(&category), &limit);

        env.events().publish((symbol_short!("cat_lim"), category), limit);
        Ok(())
    }

    pub fn get_category_limit(env: Env, category: Symbol) -> i128 {
        get_category_limit(&env, &category)
    }

    pub fn set_period_duration(
        env: Env,
        admin: Address,
        duration: u64,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let cfg = load_config(&env)?;
        if cfg.admin != admin {
            return Err(ContractError::UnauthorisedAdmin);
        }
        if duration == 0 {
            return Err(ContractError::FutureExpiryRequired);
        }
        env.storage().instance().set(&PERIOD_DURATION, &duration);
        env.events()
            .publish((symbol_short!("period"),), duration);
        Ok(())
    }

    pub fn get_period_duration(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&PERIOD_DURATION)
            .unwrap_or(86400)
    }

    pub fn set_governor(env: Env, caller: Address, governor: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let cfg = load_config(&env)?;
        if caller != cfg.admin {
            return Err(ContractError::UnauthorisedAdmin);
        }
        env.storage().instance().set(&GOVERNOR, &governor);
        Ok(())
    }

    pub fn get_governor(env: Env) -> Option<Address> {
        env.storage().instance().get(&GOVERNOR)
    }

    pub fn governor_spend(
        env: Env,
        caller: Address,
        asset: Address,
        to: Address,
        amount: i128,
        category: Symbol,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        let governor: Address = env
            .storage()
            .instance()
            .get(&GOVERNOR)
            .ok_or(ContractError::NotAuthorizedCaller)?;
        if caller != governor {
            return Err(ContractError::NotAuthorizedCaller);
        }

        if amount <= 0 {
            return Err(ContractError::PositiveAmountRequired);
        }

        let limit = get_asset_limit(&env, &asset);
        if amount > limit {
            return Err(ContractError::LimitExceeded);
        }

        let category_limit = get_category_limit(&env, &category);
        let now = env.ledger().timestamp();
        let period_start = get_period_start(&env, now);
        let spent = get_category_spent(&env, &category, period_start);
        if spent.saturating_add(amount) > category_limit {
            return Err(ContractError::LimitExceeded);
        }

        token::Client::new(&env, &asset).transfer(&env.current_contract_address(), &to, &amount);

        let key = allocation_key(&asset, &category);
        let mut total_spent: i128 = env.storage().instance().get(&key).unwrap_or(0);
        total_spent = total_spent.saturating_add(amount);
        env.storage().instance().set(&key, &total_spent);

        let spent_key = category_spent_key(&category, period_start);
        let mut period_spent: i128 = env.storage().instance().get(&spent_key).unwrap_or(0);
        period_spent = period_spent.saturating_add(amount);
        env.storage().instance().set(&spent_key, &period_spent);

        env.events()
            .publish((symbol_short!("gov_spend"), asset.clone()), (to, amount));

        Ok(())
    }

    pub fn create_transfer_proposal(
        env: Env,
        proposer: Address,
        asset: Address,
        to: Address,
        amount: i128,
        category: Symbol,
        description: String,
        expires_at: u64,
    ) -> Result<Proposal, ContractError> {
        proposer.require_auth();

        if amount <= 0 {
            return Err(ContractError::PositiveAmountRequired);
        }

        if !is_signer(&env, &proposer)? {
            return Err(ContractError::UnauthorisedProposer);
        }

        let now = env.ledger().timestamp();
        if expires_at <= now {
            return Err(ContractError::FutureExpiryRequired);
        }

        let limit = get_asset_limit(&env, &asset);
        if amount > limit {
            return Err(ContractError::LimitExceeded);
        }

        let kind = ProposalKind::Transfer(TransferProposal {
            asset: asset.clone(),
            to,
            amount,
        });
        Self::save_new_proposal(env, proposer, kind, category, description, expires_at)
    }

    pub fn create_swap_proposal(
        env: Env,
        proposer: Address,
        path: Vec<Address>,
        amount_in: i128,
        min_out: i128,
        category: Symbol,
        description: String,
        expires_at: u64,
    ) -> Result<Proposal, ContractError> {
        proposer.require_auth();

        if amount_in <= 0 || min_out <= 0 {
            return Err(ContractError::PositiveAmountRequired);
        }

        if path.len() < 2 {
            return Err(ContractError::InvalidPath);
        }

        if !is_signer(&env, &proposer)? {
            return Err(ContractError::UnauthorisedProposer);
        }

        let now = env.ledger().timestamp();
        if expires_at <= now {
            return Err(ContractError::FutureExpiryRequired);
        }

        let asset_in = path.get(0).unwrap();
        let limit = get_asset_limit(&env, &asset_in);
        if amount_in > limit {
            return Err(ContractError::LimitExceeded);
        }

        let kind = ProposalKind::Swap(SwapProposal {
            path,
            amount_in,
            min_out,
        });
        Self::save_new_proposal(env, proposer, kind, category, description, expires_at)
    }

    fn save_new_proposal(
        env: Env,
        proposer: Address,
        kind: ProposalKind,
        category: Symbol,
        description: String,
        expires_at: u64,
    ) -> Result<Proposal, ContractError> {
        let id = next_proposal_id(&env);

        let mut approvals = Vec::new(&env);
        approvals.push_back(proposer.clone());

        let proposal = Proposal {
            id,
            proposer: proposer.clone(),
            kind: kind.clone(),
            category,
            description,
            approvals,
            status: ProposalStatus::Pending,
            created_at: env.ledger().timestamp(),
            expires_at,
        };

        env.storage().persistent().set(&proposal_key(id), &proposal);

        match kind {
            ProposalKind::Transfer(t) => {
                env.events().publish(
                    (
                        symbol_short!("proposal"),
                        symbol_short!("transfer"),
                        t.asset,
                    ),
                    (id, proposer, t.amount),
                );
            }
            ProposalKind::Swap(s) => {
                let asset_sync = s.path.get(0).unwrap();
                env.events().publish(
                    (symbol_short!("proposal"), symbol_short!("swap"), asset_sync),
                    (id, proposer, s.amount_in),
                );
            }
        }
        Ok(proposal)
    }

    pub fn get_proposal(env: Env, id: u64) -> Option<Proposal> {
        let mut proposal: Proposal = env.storage().persistent().get(&proposal_key(id))?;
        if matches!(proposal.status, ProposalStatus::Pending)
            && env.ledger().timestamp() >= proposal.expires_at
        {
            proposal.status = ProposalStatus::Expired;
        }
        Some(proposal)
    }

    pub fn approve_proposal(env: Env, signer: Address, id: u64) -> Result<(), ContractError> {
        signer.require_auth();

        if !is_signer(&env, &signer)? {
            return Err(ContractError::UnauthorisedSigner);
        }

        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&proposal_key(id))
            .ok_or(ContractError::ProposalNotFound)?;

        if !matches!(proposal.status, ProposalStatus::Pending) {
            return Err(ContractError::ProposalNotPending);
        }

        let now = env.ledger().timestamp();
        if now >= proposal.expires_at {
            proposal.status = ProposalStatus::Expired;
            env.storage().persistent().set(&proposal_key(id), &proposal);
            return Err(ContractError::ProposalExpired);
        }

        if has_approval(&env, &proposal, &signer) {
            return Ok(());
        }

        proposal.approvals.push_back(signer.clone());
        env.storage().persistent().set(&proposal_key(id), &proposal);

        env.events()
            .publish((symbol_short!("approve"), id, signer), ());

        Ok(())
    }

    pub fn execute_proposal(env: Env, signer: Address, id: u64) -> Result<(), ContractError> {
        signer.require_auth();

        if !is_signer(&env, &signer)? {
            return Err(ContractError::UnauthorisedSigner);
        }

        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&proposal_key(id))
            .ok_or(ContractError::ProposalNotFound)?;

        if !matches!(proposal.status, ProposalStatus::Pending) {
            return Err(ContractError::ProposalNotPending);
        }

        let now = env.ledger().timestamp();
        if now >= proposal.expires_at {
            proposal.status = ProposalStatus::Expired;
            env.storage().persistent().set(&proposal_key(id), &proposal);
            return Err(ContractError::ProposalExpired);
        }

        let cfg = load_config(&env)?;
        let approvals = count_approvals(&proposal);
        if approvals < cfg.threshold {
            return Err(ContractError::InsufficientApprovals);
        }

        match &proposal.kind {
            ProposalKind::Transfer(t) => {
                let limit = get_asset_limit(&env, &t.asset);
                if t.amount > limit {
                    return Err(ContractError::LimitExceeded);
                }

                let category_limit = get_category_limit(&env, &proposal.category);
                let period_start = get_period_start(&env, now);
                let spent = get_category_spent(&env, &proposal.category, period_start);
                if spent.saturating_add(t.amount) > category_limit {
                    return Err(ContractError::LimitExceeded);
                }

                let token_client = token::Client::new(&env, &t.asset);
                token_client.transfer(&env.current_contract_address(), &t.to, &t.amount);

                let key = allocation_key(&t.asset, &proposal.category);
                let mut total_spent: i128 = env.storage().instance().get(&key).unwrap_or(0);
                total_spent = total_spent.saturating_add(t.amount);
                env.storage().instance().set(&key, &total_spent);

                let spent_key = category_spent_key(&proposal.category, period_start);
                let mut period_spent: i128 = env.storage().instance().get(&spent_key).unwrap_or(0);
                period_spent = period_spent.saturating_add(t.amount);
                env.storage().instance().set(&spent_key, &period_spent);

                env.events().publish(
                    (
                        symbol_short!("executed"),
                        symbol_short!("transfer"),
                        t.asset.clone(),
                    ),
                    (id, t.to.clone(), t.amount),
                );
            }
            ProposalKind::Swap(s) => {
                let asset_in = s.path.get(0).unwrap();
                let limit = get_asset_limit(&env, &asset_in);
                if s.amount_in > limit {
                    return Err(ContractError::LimitExceeded);
                }

                let category_limit = get_category_limit(&env, &proposal.category);
                let period_start = get_period_start(&env, now);
                let spent = get_category_spent(&env, &proposal.category, period_start);
                if spent.saturating_add(s.amount_in) > category_limit {
                    return Err(ContractError::LimitExceeded);
                }

                let router = cfg
                    .dex_router
                    .ok_or(ContractError::DexRouterNotConfigured)?;

                // Authorize Router to spend amount_in.
                let token_client = token::Client::new(&env, &asset_in);
                token_client.approve(
                    &env.current_contract_address(),
                    &router,
                    &s.amount_in,
                    &(env.ledger().sequence() + 100),
                );

                let args = soroban_sdk::vec![
                    &env,
                    env.current_contract_address().into_val(&env),
                    s.path.into_val(&env),
                    s.amount_in.into_val(&env),
                    s.min_out.into_val(&env)
                ];

                let res: i128 = env.invoke_contract(&router, &Symbol::new(&env, "swap"), args);

                if res < s.min_out {
                    return Err(ContractError::SwapFailed);
                }

                let asset_out = s.path.get(s.path.len() - 1).unwrap();

                let key = allocation_key(&asset_in, &proposal.category);
                let mut total_spent: i128 = env.storage().instance().get(&key).unwrap_or(0);
                total_spent = total_spent.saturating_add(s.amount_in);
                env.storage().instance().set(&key, &total_spent);

                let spent_key = category_spent_key(&proposal.category, period_start);
                let mut period_spent: i128 = env.storage().instance().get(&spent_key).unwrap_or(0);
                period_spent = period_spent.saturating_add(s.amount_in);
                env.storage().instance().set(&spent_key, &period_spent);

                env.events().publish(
                    (
                        symbol_short!("executed"),
                        symbol_short!("swap"),
                        asset_in.clone(),
                    ),
                    (id, s.amount_in, asset_out.clone(), res),
                );
            }
        }

        proposal.status = ProposalStatus::Executed;
        env.storage().persistent().set(&proposal_key(id), &proposal);

        Ok(())
    }

    pub fn get_allocation_for_category(
        env: Env,
        asset: Address,
        category: Symbol,
    ) -> AllocationSummary {
        let key = allocation_key(&asset, &category);
        let spent: i128 = env.storage().instance().get(&key).unwrap_or(0);
        AllocationSummary {
            category,
            total_spent: spent,
        }
    }

    pub fn get_category_spent_in_period(
        env: Env,
        category: Symbol,
    ) -> i128 {
        let now = env.ledger().timestamp();
        let period_start = get_period_start(&env, now);
        get_category_spent(&env, &category, period_start)
    }
}
