#![forbid(unsafe_code)]

use std::process::ExitCode;

use imessage_exporter::{
    Config,
    app::{
        call_logs::{self, CallLogFormat},
        options::{OPTION_CALL_LOG_FORMAT, OPTION_CALL_LOGS, Options, from_command_line},
    },
};

fn main() -> ExitCode {
    // Get args from command line
    let args = from_command_line();

    // `--call-log-format` only makes sense alongside `--call-logs`.
    let call_log_format = match args.get_one::<String>(OPTION_CALL_LOG_FORMAT) {
        Some(_) if !args.get_flag(OPTION_CALL_LOGS) => {
            eprintln!(
                "Invalid command line options: --{OPTION_CALL_LOG_FORMAT} requires --{OPTION_CALL_LOGS}"
            );
            return ExitCode::FAILURE;
        }
        Some(value) => match value.as_str() {
            "html" => CallLogFormat::Html,
            "pdf" => CallLogFormat::Pdf,
            _ => CallLogFormat::Csv,
        },
        None => CallLogFormat::Csv,
    };

    // Create application options
    let options = Options::from_args(&args);

    // Create app state and start
    match options {
        Ok(options) => {
            if options.export_call_logs {
                match call_logs::export_from_options(&options, call_log_format) {
                    Ok((path, result)) => {
                        println!(
                            "Exported {} call log{} from {} to {}",
                            result.entries.len(),
                            if result.entries.len() == 1 { "" } else { "s" },
                            result.source,
                            path.display()
                        );
                        return ExitCode::SUCCESS;
                    }
                    Err(why) => {
                        eprintln!("Unable to export: {why}");
                        return ExitCode::FAILURE;
                    }
                }
            }

            match Config::new(options) {
                Ok(mut app) => {
                    // Resolve the filtered contacts, if provided
                    app.resolve_filtered_handles();

                    if let Err(why) = app.start() {
                        eprintln!("Unable to export: {why}");
                        return ExitCode::FAILURE;
                    }
                    ExitCode::SUCCESS
                }
                Err(why) => {
                    eprintln!("Invalid configuration: {why}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(why) => {
            eprintln!("Invalid command line options: {why}");
            ExitCode::FAILURE
        }
    }
}
