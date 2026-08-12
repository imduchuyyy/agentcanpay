use thiserror::Error;

#[derive(Debug, Error)]
pub enum TxError {
    #[error("no built-in RPC endpoint for chain {0}; pass --rpc-url")]
    NoEndpoint(u64),

    #[error("`{0}` is not a usable RPC URL")]
    BadUrl(String),

    /// Guards against broadcasting to the wrong network: a transfer signed
    /// for one chain is a valid transaction to submit on another.
    #[error("that RPC endpoint serves chain {actual}, not chain {expected}")]
    ChainMismatch { expected: u64, actual: u64 },

    #[error("could not reach the RPC endpoint: {0}")]
    Rpc(String),

    #[error("`{0}` is not a valid amount")]
    BadAmount(String),

    #[error("amount must be greater than zero")]
    ZeroAmount,

    #[error("no ERC-20 token found at {0} on this chain")]
    NotAToken(String),

    #[error("balance is {have} {symbol}, but {want} was requested")]
    InsufficientFunds {
        symbol: String,
        have: String,
        want: String,
    },

    /// Native value competes with gas, so spending a whole balance fails
    /// at the node with an opaque message unless it is caught here.
    #[error("{want} {symbol} plus about {fee} {symbol} of gas exceeds the balance of {have}")]
    InsufficientForGas {
        symbol: String,
        have: String,
        want: String,
        fee: String,
    },

    #[error("the network rejected the transaction: {0}")]
    Rejected(String),
}
