use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=BUILD_SVUI");

    if env::var("BUILD_SVUI").as_deref() != Ok("1") {
        return;
    }

    println!("cargo:rerun-if-changed=ui/svui/src");
    println!("cargo:rerun-if-changed=ui/svui/svelte.config.js");
    println!("cargo:rerun-if-changed=ui/svui/vite.config.ts");
    println!("cargo:rerun-if-changed=ui/svui/package.json");

    let svui_dir = Path::new("ui/svui");

    let status = Command::new("pnpm")
        .args(["build"])
        .current_dir(svui_dir)
        .status()
        .expect("failed to run `pnpm build` in ui/svui — is pnpm installed?");

    if !status.success() {
        panic!("svui build failed");
    }
}
