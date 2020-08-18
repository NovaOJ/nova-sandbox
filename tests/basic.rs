use nova_sandbox::*;
use std::process::Stdio;

#[test]
fn test_pid() {
    pretty_env_logger::init();
    let sandbox = Sandbox::new(
        "/work/package/debs/debian-rootfs",
        "/home/woshiluo/tmp/test",
        "/home/woshiluo/tmp/qwq",
    )
    .unwrap();
    let config = SandboxConfig::new(
        10000,
        100 * 1024 * 1024,
        10,
        ":(){ :|: & };:".to_string(),
        Stdio::inherit(),
        Stdio::inherit(),
        Stdio::inherit(),
    );
    sandbox.run(config).unwrap();
}
