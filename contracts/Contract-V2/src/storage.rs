use crate::errors::ContractError;
use crate::types::StreamV2;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

// ----------------------------------------------------------------
// DataKeyV2 — all storage keys for the V2 contract.
//
// IMPORTANT: never reorder or remove variants — Soroban
// serialises the enum discriminant as the on-chain key.
// Only ever append new variants at the bottom.
// ----------------------------------------------------------------
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DataKeyV2 {
    // -- instance() keys -----------------------------------------
    Admin,       // kept for discriminant stability (no longer written)
    Stream(u64), // individual stream record

    // Issue #400 — multi-sig admin
    AdminList, // Vec<Address> — the current signer set
    Threshold, // u32          — minimum approvals required
    // -- Stream storage ------------------------------------------
    /// Individual stream record, keyed by stream ID.
    Stream(u64),

    // -- Dust threshold ------------------------------------------
    /// Per-asset minimum stream amount. Falls back to DEFAULT_MIN_VALUE.
    MinValue(Address),
    // -- Analytics -----------------------------------------------
    UserSeen(Address),
}

/// Global stream counter.
pub const STREAM_COUNT_V2: Symbol = symbol_short!("STR_V2");
pub const V2_TVL: Symbol = symbol_short!("V2_TVL");
pub const V2_USER_COUNT: Symbol = symbol_short!("V2_USER");

// TTL constants (~5-second ledger close time)
const INSTANCE_TTL_THRESHOLD: u32 = 518_400; // ~30 days
const INSTANCE_TTL_BUMP: u32 = 535_680; // ~31 days

// ----------------------------------------------------------------
// Backward-compat single-admin bootstrap (used by init)
// ----------------------------------------------------------------

/// Seed a single-admin list with threshold = 1.
pub fn set_admin(env: &Env, admin: &Address) {
    let mut list = Vec::new(env);
    list.push_back(admin.clone());
    set_admin_list_raw(env, &list, 1);
}

/// Return the first admin (legacy helper used by existing callers).
pub fn get_admin(env: &Env) -> Address {
    bump_instance(env);
    get_admin_list(env)
        .first()
        .expect("V2: AdminList not set")
}

/// Returns true if the admin list has been initialised.
pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKeyV2::AdminList)
}

// ----------------------------------------------------------------
// Multi-sig admin storage (Issue #400)
// ----------------------------------------------------------------

/// Atomically replace the admin set and threshold.
/// Validation (threshold bounds) is enforced in lib.rs.
pub fn set_admin_list_raw(env: &Env, admins: &Vec<Address>, threshold: u32) {
    env.storage()
        .instance()
        .set(&DataKeyV2::AdminList, admins);
    env.storage()
        .instance()
        .set(&DataKeyV2::Threshold, &threshold);
    bump_instance(env);
}

/// Return the full admin list.
pub fn get_admin_list(env: &Env) -> Vec<Address> {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&DataKeyV2::AdminList)
        .expect("V2: AdminList not set")
}

/// Return the approval threshold.
pub fn get_threshold(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKeyV2::Threshold)
        .unwrap_or(1)
}

/// Require that at least `threshold` of the admin list have authorised
/// this invocation.
///
/// Callers pass `signers` — the subset of admins that are signing this
/// particular transaction. The function:
///   1. Verifies every address in `signers` is in the admin list.
///   2. Calls `require_auth()` on each (host enforces the auth entry).
///   3. Checks `signers.len() >= threshold`.
pub fn require_multisig(
    env: &Env,
    signers: &Vec<Address>,
) -> Result<(), ContractError> {
    let admins = get_admin_list(env);
    let threshold = get_threshold(env);

    // Every supplied signer must be a registered admin.
    for signer in signers.iter() {
        if !admins.contains(&signer) {
            return Err(ContractError::NotEnoughSigners);
        }
        signer.require_auth();
    }

    if signers.len() < threshold {
        return Err(ContractError::NotEnoughSigners);
    }

    Ok(())
}

// ----------------------------------------------------------------
// Stream storage
// ----------------------------------------------------------------

pub fn next_stream_id(env: &Env) -> u64 {
    let id: u64 = env.storage().instance().get(&STREAM_COUNT_V2).unwrap_or(0);
    env.storage().instance().set(&STREAM_COUNT_V2, &(id + 1));
    id
}

pub fn set_stream(env: &Env, stream_id: u64, stream: &StreamV2) {
    env.storage()
        .instance()
        .set(&DataKeyV2::Stream(stream_id), stream);
    bump_instance(env);
}

pub fn get_stream(env: &Env, stream_id: u64) -> Option<StreamV2> {
    bump_instance(env);
    env.storage().instance().get(&DataKeyV2::Stream(stream_id))
}

// ----------------------------------------------------------------
// instance() helpers — Analytics
// ----------------------------------------------------------------

/// Update TVL and unique user count.
pub fn update_stats(env: &Env, amount: i128, sender: &Address, receiver: &Address) {
    // Update TVL
    let tvl: i128 = env.storage().instance().get(&V2_TVL).unwrap_or(0);
    env.storage().instance().set(&V2_TVL, &(tvl + amount));

    // Update User Count
    let mut user_count: u32 = env.storage().instance().get(&V2_USER_COUNT).unwrap_or(0);

    if !env.storage().persistent().has(&DataKeyV2::UserSeen(sender.clone())) {
        env.storage().persistent().set(&DataKeyV2::UserSeen(sender.clone()), &true);
        user_count += 1;
    }

    if !env.storage().persistent().has(&DataKeyV2::UserSeen(receiver.clone())) {
        env.storage().persistent().set(&DataKeyV2::UserSeen(receiver.clone()), &true);
        user_count += 1;
    }

    env.storage().instance().set(&V2_USER_COUNT, &user_count);
    bump_instance(env);
}

/// Retrieve all V2 summary metrics.
pub fn get_health(env: &Env) -> crate::types::ProtocolHealthV2 {
    crate::types::ProtocolHealthV2 {
        total_v2_tvl: env.storage().instance().get(&V2_TVL).unwrap_or(0),
        active_v2_users: env.storage().instance().get(&V2_USER_COUNT).unwrap_or(0),
        total_v2_streams: env.storage().instance().get(&STREAM_COUNT_V2).unwrap_or(0),
    }
}

// ----------------------------------------------------------------
// TTL
// ----------------------------------------------------------------

pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
}

// ----------------------------------------------------------------
// Dust threshold helpers
// ----------------------------------------------------------------

/// 10 XLM in stroops (1 XLM = 10_000_000 stroops).
pub const DEFAULT_MIN_VALUE: i128 = 100_000_000; // 10 XLM

/// Set a per-asset minimum stream amount. Admin-only enforcement is
/// done in the contract layer.
pub fn set_min_value(env: &Env, asset: &Address, min: i128) {
    env.storage()
        .instance()
        .set(&DataKeyV2::MinValue(asset.clone()), &min);
    bump_instance(env);
}

/// Return the minimum stream amount for `asset`, defaulting to 10 XLM.
pub fn get_min_value(env: &Env, asset: &Address) -> i128 {
    env.storage()
        .instance()
        .get(&DataKeyV2::MinValue(asset.clone()))
        .unwrap_or(DEFAULT_MIN_VALUE)
}
