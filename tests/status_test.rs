use nova_sandbox::*;
use std::process::Stdio;

fn run_sandbox(command: String, test_id: &str) -> SandboxStatus {
    let sandbox = Sandbox::create(
        "/work/package/debs/debian-rootfs",
        &format!("/work/novaoj/nova-sandbox/tests/{}", test_id),
    )
    .unwrap();
    sandbox
        .exec(SandboxConfig {
            time_limit: 5000,
            memory_limit: 256 * 1024 * 1024,
            command,
            stdin: Stdio::null(),
            stdout: Stdio::null(),
            stderr: Stdio::inherit(),
        })
        .unwrap()
}

#[test]
fn tle() {
    let exec = |command| run_sandbox(command, "tle");
    let status = exec(String::from("g++ ./tle.cpp -o tle.run"));

    if let SandboxStatusKind::Success = status.status {
    } else {
        panic!("Failed to compile test: {:?}", status);
    }

    let status = exec(String::from("./tle.run"));

    if let SandboxStatusKind::TimeLimitExceeded = status.status {
    } else {
        panic!("Failed to exec program: {:?}", status);
    }
}

#[test]
fn mle() {
    let exec = |command| run_sandbox(command, "mle");
    let status = exec(String::from("g++ ./mle.cpp -o mle.run"));

    if let SandboxStatusKind::Success = status.status {
    } else {
        panic!("Failed to compile test: {:?}", status);
    }

    let status = exec(String::from("./mle.run"));

    if let SandboxStatusKind::MemoryLimitExceeded = status.status {
    } else {
        panic!("Failed to exec program: {:?}", status);
    }
}

#[test]
fn re() {
    let exec = |command| run_sandbox(command, "re");
    let status = exec(String::from("g++ ./re.cpp -o re.run"));

    if let SandboxStatusKind::Success = status.status {
    } else {
        panic!("Failed to compile test: {:?}", status);
    }

    let status = exec(String::from("./re.run"));

    if let SandboxStatusKind::RuntimeError = status.status {
    } else {
        panic!("Failed to exec program: {:?}", status);
    }
}

#[test]
fn check_sandbox() {
    let status = run_sandbox(String::from("echo 1 >/qwq"), "");

    if let SandboxStatusKind::RuntimeError = status.status {
    } else {
        panic!("Failed to exec program: {:?}", status);
    }
}
