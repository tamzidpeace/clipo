#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use clipo_lib::ipc::{default_ipc_endpoint, parse_cli_args, send_ipc_command, CliCommand};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!(
            "Usage: clipo [FLAG]\n\nFlags:\n  --pause        Pause clipboard monitoring\n  --resume       Resume clipboard monitoring\n  --ignore-next  Ignore the next clipboard change\n  --toggle       Toggle the Clipo popup window\n  -h, --help     Print this help message"
        );
        return ExitCode::SUCCESS;
    }

    let cli_cmd = match parse_cli_args(&args) {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("Error: {err}");
            eprintln!("Run `clipo --help` for usage.");
            return ExitCode::from(2);
        }
    };

    match cli_cmd {
        CliCommand::RunGui => {
            clipo_lib::run();
            ExitCode::SUCCESS
        }
        control_cmd => {
            let Some(ipc_cmd) = control_cmd.to_ipc_command() else {
                return ExitCode::FAILURE;
            };
            let endpoint = default_ipc_endpoint();
            if let Err(err) = send_ipc_command(&endpoint, ipc_cmd) {
                eprintln!(
                    "Failed to contact running Clipo instance at {}: {err}",
                    endpoint.display()
                );
                eprintln!("Make sure Clipo is running before issuing control commands.");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
    }
}
