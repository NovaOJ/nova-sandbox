// Here is an example.

use nova_sandbox::SandboxConfig;
use nova_sandbox::sanbox_run;
use std::process::Stdio;


fn main() {
    pretty_env_logger::init();
    log::info!( "Start Program" );
    let status = sanbox_run( SandboxConfig{ 
        rootfs_directory: "/work/package/debs/debian-rootfs/".to_string(),
        time_limit: 10000,
        memory_limit: 1024 * 1024 * 3,

        work_directory: "/home/woshiluo/tmp".to_string(),

//        command: "echo \"Hello, World\"".to_string(),
//        command: String::from( "sleep 5" ),
//        command: String::from( "ls -l /tmp" ),
//        command: String::from( "echo 1 > /qwq" ),
//        command: String::from( "rm /tmp/a.out" ),
        command: String::from( "./a.out" ),
//        command: String::from( "g++ temp.cpp" ),


        stdin: Stdio::null(),
        stdout: Stdio::inherit(),
        stderr: Stdio::inherit() } ).unwrap_or_else( |err|{
        log::error!( "Failed to run sandbox: {}", err );
        panic!( "{}", err );
    });

    log::debug!( "{:?}", status );
}
