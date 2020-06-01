//! Nova Sandbox
//!
//! 一个致力用于 OJ/判题环境 的 Sandbox
use std::process::Command;
use std::process::Stdio;
use std::error::Error;

#[derive(Debug)]
pub struct SandboxConfig {
    /// Rootfs 目录
    pub rootfs_directory: String,
    /// 时间限制（以 ms 为单位）
    pub time_limit: u32,
    /// 内存限制（以 Byte 为单位）
    pub memory_limit: u32,

    /// Work Directory  
    /// 执行时只有这个目录是可写的  
    /// Rootfs 是以**只读方式**挂载的  
    /// **Warn: 程序对这个目录有全部权限**
    pub work_directory: String,

    /// 要执行的命令
    pub command: String,

    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio
}

// enum sandbox_status_kind {
//     time_limit_exceeded,
//     memory_limit_exceeded,
//     runtime_error,
//     success
// }
// 
// struct sandbox_status {
// }

/// 根据 SandboxConfig 来执行命令
///
/// # Examples
///
/// ```
/// sanbox_run( SandboxConfig{ 
///     rootfs_directory,
///     time_limit: 1000,
///     memory_limit: 512,
/// 
///     work_directory: work_directory,
/// 
///     command: "echo \"Hello, World\"".to_string(),
/// 
///     stdin: Stdio::null(),
///     stdout: Stdio::inherit(),
///     stderr: Stdio::inherit() } ).unwrap_or_else( |err|{
///         log::error!( "Failed to run sandbox: {}", err );
///         panic!( "{}", err );
/// });
/// ```
pub fn sanbox_run( config: SandboxConfig ) -> Result< (), Box<dyn Error> > {
    use cgroups_fs::CgroupsCommandExt;
    let cgroup_name = "wsl-sandbox";

    log::info!( "Init Sandbox" );
    // Mount Rootfs
    let target_rootfs_directory = &format!( "/tmp/{}", cgroup_name );
    Command::new("mkdir")
        .args( &[ "-p", &target_rootfs_directory ] )
        .stdout( Stdio::null() )
        .spawn()?.wait()?;
    Command::new("mount")
        .args(&["-o", "ro",
              "--bind",
              &config.rootfs_directory,
              &target_rootfs_directory ])
        .stdout( Stdio::null() )
        .spawn()?.wait()?;

    // Mount WorkDirectory 
    let target_work_directory = &format!( "{}/tmp/{}", config.rootfs_directory, cgroup_name );
    Command::new("mkdir")
        .args( &[ "-p", &target_work_directory ] )
        .stdout( Stdio::null() )
        .spawn()?.wait()?;
    Command::new("mount")
        .args( &[ "--bind", &config.work_directory, &target_work_directory ] )
        .stdout( Stdio::null() )
        .spawn()?.wait()?;
    log::info!( "Done!" );

    log::info!( "New cgourps {} create", cgroup_name );
    let cur_cgroup = cgroups_fs::CgroupName::new( cgroup_name );
    let cur_cgroup = cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "memory")?;
    log::info!( "Memory {}", config.memory_limit * 2 );
    cur_cgroup.set_value( "memory.limit_in_bytes", config.memory_limit * 2 )?;

    // Run command
    let command = format!( "cd /tmp/{};{}", cgroup_name, config.command ); 
    log::debug!( "Chroot {} to run '{}'", target_rootfs_directory, command );
    let q = std::process::Command::new( "timeout" )
        .args( &[ "10", 
               "chroot", &target_rootfs_directory, 
               "bash", "-c", &command ] )
        .cgroups(&[&cur_cgroup])
//        .output()?;
        .stdin( config.stdin )
        .stdout( config.stdout )
        .stderr( config.stderr )
        .spawn()?.wait()?;
    log::debug!( "{:?}", q );


    log::info!( "Remove Sandbox" );
    // Umount rootfs
    Command::new("umount")
        .args( &[ "-R", &target_rootfs_directory ])
        .stdout( Stdio::null() )
        .spawn()?.wait()?;
    Command::new("rm")
        .args(&[ "-rf", &target_work_directory ])
        .stdout( Stdio::null() )
        .spawn()?.wait()?;
    log::info!( "Done!" );

    Ok( () )
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn hello_world() {
        sanbox_run( SandboxConfig{ 
            rootfs_directory: "/work/package/debs/debian-rootfs/".to_string(),
            time_limit: 1000,
            memory_limit: 512,

            work_directory: "/home/woshiluo/tmp".to_string(),

            command: "echo \"Hello, World\"".to_string(),

            stdin: Stdio::null(),
            stdout: Stdio::inherit(),
            stderr: Stdio::inherit() } ).unwrap_or_else( |err|{
                log::error!( "Failed to run sandbox: {}", err );
                panic!( "{}", err );
        });
    }
}
