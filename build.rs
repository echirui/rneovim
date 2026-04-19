use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("libos_terminal.a");

    // 1. .c を .o にコンパイル
    let status = Command::new("clang")
        .args(&["-c", "src/os_terminal.c", "-o"])
        .arg(format!("{}/os_terminal.o", out_dir))
        .status()
        .expect("Failed to run clang");
    assert!(status.success());

    // 2. .o を .a (スタティックライブラリ) にまとめる
    let status = Command::new("ar")
        .args(&["rcs"])
        .arg(&dest_path)
        .arg(format!("{}/os_terminal.o", out_dir))
        .status()
        .expect("Failed to run ar");
    assert!(status.success());

    // 3. Rustにリンクを指示
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=os_terminal");
}
