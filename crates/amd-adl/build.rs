fn main() {
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=vendor");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .cpp(true)
            .include("vendor")
            .file("native/adl_bridge.cpp")
            .flag_if_supported("/std:c++17")
            .flag_if_supported("-std=c++17")
            .compile("adl_bridge");
    }
}
