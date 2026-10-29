//! `ssg-sim`: the headless runner. `ssg-sim run <project-dir> [options]` runs a
//! capture scenario without a window and writes `summary.json` and
//! `trajectory.csv`; `ssg-sim validate <project-dir>` checks a project's files.

mod args;
mod inputs;
mod output;
mod scenario;
mod validate;

use std::process::ExitCode;

fn main() -> ExitCode {
    let command = match args::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("ssg-sim: {message}");
            }
            eprintln!("{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    match command {
        args::Command::Run(args) => {
            match scenario::run(&args).and_then(|run| output::write(&args, &run)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("ssg-sim: {message}");
                    ExitCode::from(1)
                }
            }
        }
        args::Command::Validate(args) => match validate::validate(&args) {
            Ok(report) => {
                for line in &report.lines {
                    println!("{line}");
                }
                ExitCode::from(u8::from(report.errors > 0))
            }
            Err(message) => {
                eprintln!("ssg-sim: {message}");
                eprintln!("{}", args::USAGE);
                ExitCode::from(2)
            }
        },
    }
}
