// Here is an example.

use nova_sandbox::SandboxConfig;
use nova_sandbox::sanbox_run;
use std::process::Stdio;


fn main() {
    pretty_env_logger::init();
    log::info!( "Start Program" );
    sanbox_run( SandboxConfig{ 
        rootfs_directory: "/work/package/debs/debian-rootfs/".to_string(),
        time_limit: 1000,
        memory_limit: 1024 * 1024 * 512,

        work_directory: "/home/woshiluo/tmp".to_string(),

        command: "./a.out".to_string(),

        stdin: Stdio::null(),
        stdout: Stdio::inherit(),
        stderr: Stdio::inherit() } ).unwrap_or_else( |err|{
        log::error!( "Failed to run sandbox: {}", err );
        panic!( "{}", err );
    });
}
