use std::env;
use std::fs;
use std::process::ExitCode;

use PiSt::interpreter::TransportValue;
use PiSt::parser::parse_source;
use PiSt::scheduler::{connect_to, execute_distributed, LineResult};
use PiSt::worker::run_worker;

struct CliArgs {
    worker_bind: Option<String>,
    sourcefile: Option<String>,
    connects: Vec<String>,
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    if let Some(bind_addr) = args.worker_bind {
        println!("Starting worker on {bind_addr}");
        match run_worker(&bind_addr) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Worker failed on {bind_addr}: {err}");
                ExitCode::FAILURE
            }
        }
    } else {
        run_scheduler_mode(&args)
    }
}

fn run_scheduler_mode(args: &CliArgs) -> ExitCode {
    let source_path = match &args.sourcefile {
        Some(path) => path,
        None => {
            eprintln!("Missing required --sourcefile in scheduler mode.");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    if args.connects.is_empty() {
        eprintln!("Provide at least one --connect IP:PORT in scheduler mode.");
        print_usage();
        return ExitCode::FAILURE;
    }

    let source = match fs::read_to_string(source_path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("Failed to read `{source_path}`: {err}");
            return ExitCode::FAILURE;
        }
    };

    let ast = match parse_source(&source) {
        Ok(items) => items,
        Err(err) => {
            eprintln!("Parse error at {}: {}", err.position, err.message);
            return ExitCode::FAILURE;
        }
    };

    let mut workers = Vec::new();
    for entry in &args.connects {
        let (ip, port) = match split_ip_port(entry) {
            Ok(v) => v,
            Err(msg) => {
                eprintln!("{msg}");
                return ExitCode::FAILURE;
            }
        };
        match connect_to(ip, port) {
            Ok(addr) => {
                println!("Connected worker: {addr}");
                workers.push(addr);
            }
            Err(err) => {
                eprintln!("Failed to connect to {entry}: {}", err.message);
                return ExitCode::FAILURE;
            }
        }
    }

    let output = match execute_distributed(&ast, &workers) {
        Ok(out) => out,
        Err(err) => {
            eprintln!("Scheduler failed: {}", err.message);
            return ExitCode::FAILURE;
        }
    };

    println!("----- DISTRIBUTED RESULT -----");
    for (idx, line) in output.line_results.iter().enumerate() {
        match line {
            LineResult::Binding { name, value } => {
                println!(
                    "line {}: {} = {}",
                    idx + 1,
                    name,
                    summarize_transport_value(value)
                );
            }
            LineResult::Expression { value } => {
                println!(
                    "line {}: expr = {}",
                    idx + 1,
                    summarize_transport_value(value)
                );
            }
        }
    }

    ExitCode::SUCCESS
}

fn summarize_transport_value(value: &TransportValue) -> String {
    match value {
        TransportValue::Nat(v) => v.to_string(),
        TransportValue::Bool(v) => v.to_string(),
        TransportValue::Closure(closure) => {
            format!(
                "<closure params={} bound={}>",
                closure.params.len(),
                closure.bound_args.len()
            )
        }
    }
}

fn parse_args() -> Result<CliArgs, String> {
    let mut worker_bind = None;
    let mut sourcefile = None;
    let mut connects = Vec::new();

    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--worker-bind" => {
                let value = it
                    .next()
                    .ok_or_else(|| "Expected value after --worker-bind".to_string())?;
                worker_bind = Some(value);
            }
            "--sourcefile" => {
                let value = it
                    .next()
                    .ok_or_else(|| "Expected value after --sourcefile".to_string())?;
                sourcefile = Some(value);
            }
            "--connect" => {
                let value = it
                    .next()
                    .ok_or_else(|| "Expected value after --connect".to_string())?;
                connects.push(value);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
    }

    if worker_bind.is_some() && (!connects.is_empty() || sourcefile.is_some()) {
        return Err(
            "Do not mix --worker-bind with --connect/--sourcefile. Choose one mode.".to_string(),
        );
    }

    Ok(CliArgs {
        worker_bind,
        sourcefile,
        connects,
    })
}

fn split_ip_port(value: &str) -> Result<(&str, u16), String> {
    let (ip, port_str) = value
        .rsplit_once(':')
        .ok_or_else(|| format!("Invalid --connect value `{value}`; expected IP:PORT"))?;
    let port = port_str
        .parse::<u16>()
        .map_err(|_| format!("Invalid port in --connect value `{value}`"))?;
    Ok((ip, port))
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  Worker mode:");
    eprintln!("    PiSt --worker-bind 0.0.0.0:43101");
    eprintln!("  Scheduler mode:");
    eprintln!(
        "    PiSt --sourcefile source.p --connect 192.168.1.50:43101 --connect 192.168.1.51:43101"
    );
}
