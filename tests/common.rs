use nova_sandbox::*;
use std::fs;
use std::process::Stdio;

pub fn run_sandbox<T: std::fmt::Display>(command: T) -> nova_sandbox::SandboxStatus {
    let work_directory = format!("/tmp/{}", uuid::Uuid::new_v4().to_string());
    let sandbox_directory = format!("/tmp/{}", uuid::Uuid::new_v4().to_string());
    fs::create_dir(&work_directory).unwrap();
    fs::create_dir(&sandbox_directory).unwrap();

    let sandbox = Sandbox::new(
        "/work/package/debs/debian-rootfs",
        &work_directory,
        &sandbox_directory,
    )
    .unwrap();

    let config = SandboxConfig::new(
        1000,
        8 * 1024 * 1024,
        5,
        command,
        Stdio::inherit(),
        Stdio::inherit(),
        Stdio::inherit(),
    );
    let status = sandbox.run(config).unwrap();

    drop(sandbox);
    fs::remove_dir_all(work_directory).unwrap();
    fs::remove_dir_all(sandbox_directory).unwrap();

    status
}
