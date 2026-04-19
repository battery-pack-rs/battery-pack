use clap::Parser;

pub fn my_completer() -> Vec<String> {
    vec!["test".to_string()]
}

#[derive(Parser)]
struct TestCmd {
    #[arg(add = clap_complete::env::ArgValueCompleter::new(my_completer))]
    arg: String,
}
fn main() {}
