use std::process::Command;

fn main() {
    let version = match (
        git_output(&["rev-list", "--count", "HEAD"]),
        git_output(&["rev-parse", "--short=7", "HEAD"]),
    ) {
        (Some(rev_count), Some(short_hash)) => format!("r{rev_count}.{short_hash}"),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    };

    println!("cargo:rustc-env=APP_VERSION={version}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|v| v.trim().to_owned())
}
