use std::path::Path;

const BINARY_NAME: &str = "lunik";

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let binary_name = args
        .first()
        .and_then(|arg0| extract_arg0_executable_name(arg0));
    if let Some(binary_name) = binary_name {
        match binary_name.as_str() {
            BINARY_NAME => multiplex(&binary_name, &args[1..]),
            _ => self_ops(&args[1..]),
        }
    } else {
        self_ops(&args)
    }
}

fn extract_arg0_executable_name(arg0: &str) -> Option<String> {
    let path = Path::new(arg0);
    path.file_stem()
        .map(|file_name| file_name.to_string_lossy().to_string())
}

fn multiplex(binary_name: &str, argv: &[String]) {
    // Check if the next argument starts with "+"
    // If it does, it specifies which version of the toolchain to use
}

fn self_ops(argv: &[String]) {}
