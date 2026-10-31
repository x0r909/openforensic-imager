use std::process::ExitCode;

fn main() -> ExitCode {
    match native_bindings::run_stdio_worker() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
