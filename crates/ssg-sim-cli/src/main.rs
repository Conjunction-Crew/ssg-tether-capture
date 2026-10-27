//! `ssg-sim`: the headless runner. `ssg-sim run <project-dir> [options]` runs a
//! capture scenario without a window and writes `summary.json` and
//! `trajectory.csv`.

mod args;
mod inputs;
mod output;
mod scenario;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = match args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("ssg-sim: {message}");
            }
            eprintln!("{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    match scenario::run(&args).and_then(|run| output::write(&args, &run)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ssg-sim: {message}");
            ExitCode::from(1)
        }
    }
}
