use std::process::{Command, ExitStatus};

fn main() {
    let script = format!("{}/scripts/run.sh", env!("CARGO_MANIFEST_DIR"));
    let status = Command::new("bash")
        .arg(script)
        .status()
        .expect("failed to execute scripts/run.sh");

    if !status.success() {
        std::process::exit(exit_code(status));
    }
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}
