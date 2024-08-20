pub fn entry(binary_name: &str, argv: &[String]) {
    // Check if the next argument starts with "+"
    // If it does, it specifies which version of the toolchain to use
    // Otherwise, we check if we have specified the toolchain in the environment variable
    let mux_toolchain = argv
        .first()
        .and_then(|arg| arg.strip_prefix('+'))
        .map(|toolchain| toolchain.to_string())
        .or_else(|| std::env::var("LUNIK_TOOLCHAIN").ok());
}
