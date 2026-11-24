#![no_std]

use common::CommonError;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};
use retinax_common as common;

pub mod channel;
pub mod dispute;
pub mod settlement;

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_signature_validation;

const VISION_RECORDS: Symbol = symbol_short!("V_REC");
const ADMIN: Symbol = symbol_short!("ADMIN");

#[contract]
pub struct StateChannelContract;

#[contractimpl]
impl StateChannelContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), CommonError> {
        if env.storage().instance().has(&ADMIN) {
            return Err(CommonError::AlreadyInitialized);
        }
        env.storage().instance().set(&ADMIN, &admin);
        env.storage()
            .instance()
            .set(&VISION_RECORDS, &vision_records);
        env.storage()
            .instance()
            .set(&symbol_short!("DISP_WIN"), &86_400u64);
        Ok(())
    }

    pub fn set_dispute_window(
        env: Env,
        admin: Address,
        window_secs: u64,
    ) -> Result<(), CommonError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .ok_or(CommonError::NotInitialized)?;
        if admin != stored_admin {
            return Err(CommonError::AccessDenied);
        }
        if window_secs == 0 {
            return Err(CommonError::InvalidInput);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("DISP_WIN"), &window_secs);
        Ok(())
    }

    pub fn get_dispute_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&symbol_short!("DISP_WIN"))
            .unwrap_or(86_400)
    }

    pub fn open_channel(
        env: Env,
        patient: Address,
        provider: Address,
        capacity: u64,
    ) -> Result<u64, CommonError> {
        patient.require_auth();
        channel::open(&env, patient, provider, capacity)
    }

    pub fn open_channel_with_timeout(
        env: Env,
        patient: Address,
        provider: Address,
        capacity: u64,
        dispute_window: u64,
    ) -> Result<u64, CommonError> {
        patient.require_auth();
        channel::open_with_timeout(&env, patient, provider, capacity, dispute_window)
    }

    pub fn cooperative_close(
        env: Env,
        channel_id: u64,
        latest_balance: u64,
        latest_nonce: u64,
        patient_sig: soroban_sdk::BytesN<64>,
        provider_sig: soroban_sdk::BytesN<64>,
    ) -> Result<(), CommonError> {
        channel::cooperative_close(
            &env,
            channel_id,
            latest_balance,
            latest_nonce,
            patient_sig,
            provider_sig,
        )
    }

    pub fn unilateral_close(env: Env, channel_id: u64, closer: Address) -> Result<(), CommonError> {
        closer.require_auth();
        dispute::unilateral_close(&env, channel_id, closer)
    }

    pub fn submit_fraud_proof(
        env: Env,
        channel_id: u64,
        invalid_state_nonce: u64,
        invalid_state_balance: u64,
        signature: soroban_sdk::BytesN<64>,
    ) -> Result<(), CommonError> {
        dispute::submit_fraud_proof(
            &env,
            channel_id,
            invalid_state_nonce,
            invalid_state_balance,
            signature,
        )
    }

    pub fn settle(env: Env, channel_id: u64) -> Result<(), CommonError> {
        settlement::settle(&env, channel_id)
    }

    pub fn rebalance(
        env: Env,
        channel_id: u64,
        new_capacity: u64,
        patient_sig: soroban_sdk::BytesN<64>,
        provider_sig: soroban_sdk::BytesN<64>,
    ) -> Result<(), CommonError> {
        channel::rebalance(&env, channel_id, new_capacity, patient_sig, provider_sig)
    }

    /// Return the accumulated penalty for fraudulent dispute submissions.
    pub fn get_fraud_penalty(env: Env, channel_id: u64) -> u64 {
        env.storage()
            .persistent()
            .get(&(symbol_short!("FRD_PEN"), channel_id))
            .unwrap_or(0)
    }

    pub fn open_multi_hop(
        env: Env,
        patient: Address,
        provider: Address,
        intermediary: Address,
        capacity: u64,
    ) -> Result<u64, CommonError> {
        patient.require_auth();
        channel::open_multi_hop(&env, patient, provider, intermediary, capacity)
    }
}
