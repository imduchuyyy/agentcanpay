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
pub struct AddressArgs {
    /// Chain whose address to print.
    #[arg(long, default_value = "evm")]
    pub chain: String,
}
