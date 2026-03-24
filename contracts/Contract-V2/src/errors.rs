use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotStreamOwner = 2,
    StreamNotMigratable = 3,
    NothingToMigrate = 4,
    InvalidSignature = 5,
    ExpiredDeadline = 6,
    InvalidNonce = 7,
    // Issue #400
    InvalidThreshold = 8,   // threshold == 0 or threshold > len(admins)
    NotEnoughSigners = 9,   // fewer than threshold admins authorised the tx
    BelowDustThreshold = 8,
}
