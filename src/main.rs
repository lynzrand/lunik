mod channel;
mod config;
mod mux;
mod self_ops;

use std::path::Path;

const BINARY_NAME: &str = "lunik";

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let binary_name = args
        .first()
        .and_then(|arg0| extract_arg0_executable_name(arg0));
    let res = if let Some(binary_name) = binary_name {
        match binary_name.as_str() {
            BINARY_NAME => self_ops::entry(),
            _ => mux::entry(&binary_name, &args[1..]),
        }
    } else {
        self_ops::entry()
    };
    match res {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn extract_arg0_executable_name(arg0: &str) -> Option<String> {
    let path = Path::new(arg0);
    path.file_stem()
        .map(|file_name| file_name.to_string_lossy().to_string())
}
