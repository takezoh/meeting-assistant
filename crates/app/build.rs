//! Gives the Windows executable its resources: version information, an icon and an application manifest. Explorer,
//! Settings > Apps, Task Manager and SmartScreen show the app's name and icon from them instead of a bare file name.
//! Other targets take nothing from here.

#[path = "windows/resources.rs"]
mod resources;

use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=windows");

    let variable = |name: &str| std::env::var(name).unwrap_or_default();
    if variable("CARGO_CFG_TARGET_OS") != "windows" {
        return;
    }
    if variable("CARGO_CFG_TARGET_ENV") != "msvc" {
        println!(
            "cargo::warning=The Windows resources are linked only by the MSVC linker; this executable has no version \
             information, icon or manifest."
        );
        return;
    }

    let number = |name: &str| {
        variable(name)
            .parse::<u16>()
            .unwrap_or_else(|_| panic!("{name} is not a number Windows can show"))
    };
    let version = variable("CARGO_PKG_VERSION");
    let executable = format!("{}.exe", variable("CARGO_PKG_NAME"));
    let product = resources::Product {
        name: "Meeting Assistant",
        version: &version,
        numbers: [
            number("CARGO_PKG_VERSION_MAJOR"),
            number("CARGO_PKG_VERSION_MINOR"),
            number("CARGO_PKG_VERSION_PATCH"),
            0,
        ],
        prerelease: !variable("CARGO_PKG_VERSION_PRE").is_empty(),
        executable: &executable,
    };
    let file = PathBuf::from(variable("OUT_DIR")).join("resources.res");
    std::fs::write(
        &file,
        resources::res_file(&product, include_str!("windows/app.manifest")),
    )
    .unwrap_or_else(|error| panic!("cannot write {}: {error}", file.display()));
    // The linker turns a .res input into the executable's resource section.
    println!("cargo::rustc-link-arg-bins={}", file.display());
}
