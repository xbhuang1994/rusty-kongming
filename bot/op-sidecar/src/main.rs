use anyhow::Result;
use serde::Deserialize;
use docopt::Docopt;
use std::str;
use op_sidecar::echo::tcp_server;
use op_sidecar::echo::tcp_client;

const USAGE: &'static str = "
Welcome to use OP-Sidecar

Usage:
    cargo <command> [<args>...]
    cargo [options]

Options:
    -h, --help       Display this message
    -V, --version    Print version info and exit
    --list           List installed commands
    -v, --verbose    Use verbose output

Some sidecar commands are:
    console    Run console client
    server     Run sidecar server (only for debug)
    other      Waiting for implement
";

#[derive(Debug, Deserialize)]
struct Args {
    arg_command: Option<Command>,
    arg_args: Vec<String>,
    flag_list: bool,
    flag_verbose: bool,
}

#[derive(Debug, Deserialize)]
enum Command {
    Console,
    Server,
    Other,
}


#[tokio::main]
async fn main() {

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.options_first(true).deserialize())
        .unwrap_or_else(|e| e.exit());

    let command = match args.arg_command {
        Some(cmd) => {
            cmd
        },
        None => {
            Command::Other
        }
    };
    let mut address = String::from("127.0.0.1:12321");
    match command {
        Command::Server | Command::Console => {
            if args.arg_args.len() > 0 {
                address = args.arg_args[0].clone();
            }
        },
        _ => {},
    }

    match command {
        Command::Server => {
            tcp_server::start_sidecar_server_at_address(address).await.unwrap();
            loop {}
        },
        Command::Console => {
            tcp_client::start_sidecar_client(address).await.unwrap();
            println!("Exit!");
        },
        Command::Other => {
            println!("Command not exists {:?}", command);
        }
    }
}