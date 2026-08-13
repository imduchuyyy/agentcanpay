use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "agentcanpay", version, about = "Wallet for AI agents")]
pub struct Cli {
    /// Emit machine-readable JSON on stdout and stderr.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Set up the wallet. Opens a page where the user creates a new
    /// recovery phrase or imports one they already have.
    Create(CreateArgs),

    /// Print the wallet address.
    Address(AddressArgs),

    /// Show the recovery phrase to the user. Opens a page where they can
    /// reveal it; the phrase is never printed here.
    Reveal(RevealArgs),

    /// List what the wallet holds, across supported chains.
    Balance(BalanceArgs),

    /// List the chains this CLI can work with.
    Chains(ChainsArgs),

    /// Send tokens or native currency to another address.
    Transfer(TransferArgs),

    /// Replace this binary with the newest published release.
    Update(UpdateArgs),
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Report whether a newer release exists without installing anything.
    #[arg(long)]
    pub check: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum BackendArg {
    /// OS credential store: Keychain, Credential Manager, Secret Service.
    Keychain,
    /// Plaintext file, for hosts with no credential store.
    File,
}

impl From<BackendArg> for acp_keystore::Backend {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Keychain => Self::Keychain,
            BackendArg::File => Self::File,
        }
    }
}

/// Only options an agent can actually reason about belong here.
///
/// Whether this becomes a new wallet or an imported one, and how long the
/// phrase is, are the user's choices and are made in the browser: the agent
/// calling this has no basis for either, and must not have to guess.
#[derive(Args)]
pub struct CreateArgs {
    /// Replace an existing wallet.
    #[arg(long)]
    pub force: bool,

    /// Where to keep the recovery phrase.
    #[arg(long, value_enum, default_value_t = BackendArg::Keychain)]
    pub keystore: BackendArg,

    /// Print the setup URL instead of opening a browser, for headless hosts.
    #[arg(long)]
    pub print_url: bool,

    /// Seconds to wait for the user to finish in the browser.
    #[arg(long, default_value_t = 600)]
    pub timeout: u64,
}

#[derive(Args)]
pub struct RevealArgs {
    /// Print the URL instead of opening a browser, for headless hosts.
    #[arg(long)]
    pub print_url: bool,

    /// Seconds to wait for the user to finish with the page.
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Args)]
pub struct ChainsArgs {
    /// Include chains this wallet cannot use, such as Bitcoin and Solana.
    #[arg(long)]
    pub all: bool,
}

#[derive(Args)]
pub struct BalanceArgs {
    /// Chain to check, by id or name. Repeatable. Defaults to every
    /// supported chain, which takes several seconds.
    #[arg(long)]
    pub chain: Vec<String>,

    /// Hide holdings worth less than this many USD.
    #[arg(long, default_value_t = 0.0)]
    pub min_usd: f64,
}

/// Every field here is something the agent was told by the user, so unlike
/// `create` there is nothing for a browser page to decide. The identifiers
/// are exactly what `balance` and `chains` print.
#[derive(Args)]
pub struct TransferArgs {
    /// Chain to send on, by id or name.
    #[arg(long)]
    pub chain: String,

    /// Recipient address.
    #[arg(long)]
    pub to: String,

    /// Amount in whole tokens, as a decimal string: `1.5`, not `1500000`.
    #[arg(long)]
    pub amount: String,

    /// Token to send, by contract address, as printed by `balance`.
    /// Defaults to the chain's native currency.
    #[arg(long)]
    pub token: Option<String>,

    /// RPC endpoint to broadcast through. Defaults to a built-in public
    /// one for the chain, which may rate-limit.
    #[arg(long)]
    pub rpc_url: Option<String>,

    /// Return as soon as the transaction is broadcast, without waiting to
    /// see whether it succeeded.
    #[arg(long)]
    pub no_wait: bool,

    /// Seconds to wait for the transaction to be mined. On expiry the hash
    /// is still reported, with status `pending`.
    #[arg(long, default_value_t = 120)]
    pub timeout: u64,
}

#[derive(Args)]
pub struct AddressArgs {
    /// Chain whose address to print.
    #[arg(long, default_value = "evm")]
    pub chain: String,
}
