fn main() {
    let tsk = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("tsk")
        .expect("The Sleuth Kit development package (pkg-config name: tsk) is required");
    cc::Build::new()
        .file("src/tsk_shim.c")
        .includes(tsk.include_paths)
        .warnings(true)
        .flag_if_supported("-std=c11")
        .compile("ofi_tsk_shim");
    println!("cargo:rerun-if-changed=src/tsk_shim.c");
    println!("cargo:rerun-if-changed=src/tsk_shim.h");
}
