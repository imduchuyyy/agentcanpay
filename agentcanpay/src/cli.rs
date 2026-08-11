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
    /// Create a wallet with a newly generated recovery phrase.
    Create(CreateArgs),

    /// Import a wallet from an existing recovery phrase on stdin.
    Import(ImportArgs),

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

/// Only the two common BIP-39 lengths are offered; the intermediate sizes
/// are legal but nothing else in the ecosystem uses them.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum WordsArg {
    #[value(name = "12")]
    Twelve,
    #[value(name = "24")]
    TwentyFour,
}

impl From<WordsArg> for acp_wallet::WordCount {
    fn from(w: WordsArg) -> Self {
        match w {
            WordsArg::Twelve => Self::Twelve,
            WordsArg::TwentyFour => Self::TwentyFour,
        }
    }
}

#[derive(Args)]
pub struct CreateArgs {
    /// Length of the generated recovery phrase.
    #[arg(long, value_enum, default_value_t = WordsArg::TwentyFour)]
    pub words: WordsArg,

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
pub struct ImportArgs {
    /// Replace an existing wallet.
    #[arg(long)]
    pub force: bool,

    /// Where to keep the recovery phrase.
    #[arg(long, value_enum, default_value_t = BackendArg::Keychain)]
    pub keystore: BackendArg,
}

#[derive(Args)]
pub struct AddressArgs {
    /// Chain whose address to print.
    #[arg(long, default_value = "evm")]
    pub chain: String,
}
