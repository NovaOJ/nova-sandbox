//! Nova Sandbox
//!
//! 一个致力用于 OJ/判题环境 的 Sandbox
use std::process::Command;
use std::process::Stdio;
use std::error::Error;

use time::prelude::*;

#[derive(Debug)]
pub struct SandboxConfig {
    /// Rootfs 目录
    pub rootfs_directory: String,
    /// 时间限制（以 ms 为单位）
    pub time_limit: u32,
    /// 内存限制（以 Byte 为单位）
    pub memory_limit: u64,

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

#[derive(Debug)]
pub enum SandboxStatusKind {
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    Success
// tle > mle > re > seccess
}

#[derive(Debug)]
pub struct SandboxStatus {
    status: SandboxStatusKind,
    used_time: i128,
    max_memory: u64,
    return_code: i32
}

/// 根据 SandboxConfig 来执行命令
///
/// # Examples
///
/// ```
/// sanbox_run( SandboxConfig{ 
///     rootfs_directory,
///     time_limit: 1000,
///     memory_limit: 512 * 1024 * 1024,
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
pub fn sanbox_run( config: SandboxConfig ) -> Result< SandboxStatus, Box<dyn Error> > {
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

    // Create new cgroup
    log::info!( "New cgroup {} create", cgroup_name );
    let cur_cgroup = cgroups_fs::CgroupName::new( cgroup_name );
    let cur_cgroup = cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "memory")?;
    log::info!( "Memory Limit {}", config.memory_limit * 2 );
    cur_cgroup.set_value( "memory.limit_in_bytes", config.memory_limit * 2 )?;

    // Run command
    let command = format!( "cd /tmp/{};{}", cgroup_name, config.command ); 
    log::debug!( "Chroot {} to run '{}'", target_rootfs_directory, command );

    let time_start = time::Instant::now();
    let return_code = std::process::Command::new( "timeout" )
        .args( &[ "10", 
               "chroot", &target_rootfs_directory, 
               "bash", "-c", &command ] )
        .cgroups(&[&cur_cgroup])
        .stdin( config.stdin )
        .stdout( config.stdout )
        .stderr( config.stderr )
        .status()?.code();
    let time_end = time::Instant::now();

    let mut status = SandboxStatusKind::Success;

    log::debug!( "Return code: {:?}", return_code );
    log::debug!( "Start at {:?}, End at {:?}, Use at {:?}", time_start, time_end, time_end - time_start );

    // Get return code
    let return_code = match return_code {
        Some(code) => code,
        None => { return Err( String::from( "Process terminated by signal" ).into() ); }
    };
    if return_code != 0 {
        status = SandboxStatusKind::RuntimeError;
    }

    // Calc Memory 
    let max_memory = cur_cgroup.get_value::<u64>("memory.max_usage_in_bytes").unwrap();
    if max_memory > config.memory_limit {
        status = SandboxStatusKind::MemoryLimitExceeded;
    }

    // Calc time
    let used_time = time_end - time_start;
    log::debug!( "{:?}", used_time );
    if used_time > config.time_limit.milliseconds() {
        status = SandboxStatusKind::TimeLimitExceeded;
    }
    let used_time = used_time.whole_milliseconds();

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

    Ok( SandboxStatus{
        status,
        max_memory,
        used_time,
        return_code
    } )
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn hello_world() {
        sanbox_run( SandboxConfig{ 
            rootfs_directory: "/work/package/debs/debian-rootfs/".to_string(),
            time_limit: 1000,
            memory_limit: 512 * 1024 * 1024,

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
