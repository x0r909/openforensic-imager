fn main() {
    // Set by scripts/run-sanitizers.sh so the crate can tell an instrumented
    // build apart from a normal one; see ResourceLimits::parser_default.
    println!("cargo::rustc-check-cfg=cfg(ofi_sanitizer)");
}
