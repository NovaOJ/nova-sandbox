use nova_sandbox::*;
use std::process::Stdio;

#[test]
fn test_pid() {
    pretty_env_logger::init();
    let sandbox = Sandbox::create(
        "/work/package/debs/debian-rootfs",
        "/home/woshiluo/tmp/test",
        "/home/woshiluo/tmp/qwq",
    )
    .unwrap();
    let config = SandboxConfig::create(
        1000,
        10 * 1024 * 1024,
        4,
        "echo $$".to_string(),
        Stdio::inherit(),
        Stdio::inherit(),
        Stdio::inherit(),
    );
    sandbox.run(config).unwrap();
}