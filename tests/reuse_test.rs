use nova_sandbox::*;
use std::process::Stdio;

#[test]
fn reuse() {
    let sandbox = Sandbox::create(
        "/work/package/debs/debian-rootfs",
        "/work/novaoj/nova-sandbox/tests/reuse",
    )
    .unwrap();
    let config = SandboxConfig {
        time_limit: 5000,
        memory_limit: 256 * 1024 * 1024,
        command: "ls",
        pids_limit: 8,
        stdin: Stdio::null(),
        stdout: Stdio::null(),
        stderr: Stdio::inherit(),
    };

    sandbox.exec(config).unwrap();
    let config = SandboxConfig {
        time_limit: 5000,
        memory_limit: 256 * 1024 * 1024,
        command: "ls",
        pids_limit: 8,
        stdin: Stdio::null(),
        stdout: Stdio::null(),
        stderr: Stdio::inherit(),
    };
    sandbox.exec(config).unwrap();
}
