use argh::FromArgs;

#[derive(FromArgs, PartialEq, Debug)]
/// Time Tracker
pub struct Args {
    /// use in-memory database
    #[argh(switch)]
    pub memory: bool,

    #[argh(subcommand)]
    pub subcommand: Option<SubCommand>,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum SubCommand {
    Add(AddArgs),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "add")]
/// Add a process to track
pub struct AddArgs {
    #[argh(positional)]
    /// name of the process
    pub process_name: String,
    #[argh(positional)]
    /// path to the process
    pub path: String,
}
