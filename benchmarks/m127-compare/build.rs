use std::{env, path::PathBuf, process::Command};

fn main() {
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ARCH").unwrap(),
        "x86_64",
        "this comparison selects FourQ x64 C"
    );
    let root = PathBuf::from(
        env::var_os("FOURQ_ROOT").expect("set FOURQ_ROOT to pinned FourQlib checkout"),
    );
    let rev = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .expect("git");
    assert!(rev.status.success());
    assert_eq!(
        String::from_utf8(rev.stdout).unwrap().trim(),
        "1031567f23278e1135b35cc04e5d74c2ac88c029"
    );
    let dirty = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "status",
            "--porcelain",
            "--untracked-files=no",
        ])
        .output()
        .unwrap();
    assert!(
        dirty.status.success() && dirty.stdout.is_empty(),
        "FourQ source must be unchanged"
    );
    cc::Build::new()
        .file("fourq_batch.c")
        .include(root.join("FourQ_64bit_and_portable"))
        .define("_AMD64_", None)
        .define("__LINUX__", None)
        .flag("-O3")
        .flag("-march=native")
        .flag("-fwrapv")
        .flag("-fno-strict-aliasing")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .warnings(false)
        .compile("fourq_compare_x64_c");
    println!("cargo:rerun-if-env-changed=FOURQ_ROOT");
    println!("cargo:rerun-if-changed=fourq_batch.c");
    println!("cargo:rerun-if-changed=fourq_kernel.h");
    for file in [
        "eccp2_core.c",
        "AMD64/fp_x64.h",
        "FourQ_internal.h",
        "FourQ.h",
        "FourQ_params.h",
        "FourQ_tables.h",
        "table_lookup.h",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            root.join("FourQ_64bit_and_portable").join(file).display()
        );
    }
    println!("cargo:rustc-env=FOURQ_COMMIT=1031567f23278e1135b35cc04e5d74c2ac88c029");
}
