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
    /// Create a wallet by signing a message with a browser wallet.
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

#[derive(Args)]
pub struct CreateArgs {
    /// Derivation index; bump this to move to a fresh wallet from the same
    /// browser key.
    #[arg(long, default_value_t = 0)]
    pub index: u32,

    /// Replace an existing wallet.
    #[arg(long)]
    pub force: bool,

    /// Where to keep the recovery phrase.
    #[arg(long, value_enum, default_value_t = BackendArg::Keychain)]
    pub keystore: BackendArg,

    /// Print the URL instead of opening a browser, for headless hosts.
    #[arg(long)]
    pub print_url: bool,

    /// Seconds to wait for the browser handshake.
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Args)]
pub struct AddressArgs {
    /// Chain whose address to print.
    #[arg(long, default_value = "evm")]
    pub chain: String,
}
