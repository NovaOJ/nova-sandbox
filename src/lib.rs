//! Nova Sandbox
//!
//! 一个致力用于 OJ/判题环境 的 Sandbox
use std::error::Error;
use std::ffi::OsStr;
use std::os::unix::process::CommandExt;
use std::process::Stdio;

// use time::prelude::*;

/// Sandbox 的配置
#[derive(Debug)]
pub struct SandboxConfig<'a> {
    /// 时间限制（以 ms 为单位）
    pub time_limit: u64,
    /// 内存限制（以 Byte 为单位）
    pub memory_limit: u64,

    /// 要执行的命令
    pub command: &'a str,

    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
}

/// Sandbox 运行状态种类
#[derive(Debug)]
pub enum SandboxStatusKind {
    /// 超时
    TimeLimitExceeded,
    /// 内存超限
    MemoryLimitExceeded,
    /// 运行时错误/返回值非 0
    RuntimeError,
    /// 正常
    Success, // tle > mle > re > seccess
}

/// 沙箱运行状态
#[derive(Debug)]
pub struct SandboxStatus {
    /// 分类
    pub status: SandboxStatusKind,
    /// 使用时间
    pub used_time: u128,
    /// 使用内存
    pub max_memory: u64,
    /// 程序返回值
    pub return_code: i32,
}

pub struct Sandbox {
    /// Sandbox ID
    pub sandbox_id: String,

    /// Rootfs 目录
    pub rootfs_directory: std::path::PathBuf,
    /// Work Directory  
    /// 执行时只有这个目录是可写的  
    /// Rootfs 是以**只读方式**挂载的  
    /// 这个目录的父级应当和这个目录在同一文件系统内，否则无法启动沙箱
    /// **Warn: 程序对这个目录有全部权限**
    /// **Warn: Do not set "/tmp" value for this var**
    pub work_directory: std::path::PathBuf,

    /// 沙箱挂载点
    sandbox_directory: std::path::PathBuf,
    /// 管理内存的 Cgroup
    pub cur_cgroup: cgroups_fs::AutomanagedCgroup,
}

impl Sandbox {
    //{{{
    pub fn create<T, U>(rootfs_directory: T, work_directory: U) -> Result<Sandbox, Box<dyn Error>>
    where
        T: AsRef<std::path::Path>,
        U: AsRef<std::path::Path>,
    {
        //{{{
        let sandbox_id = uuid::Uuid::new_v4().to_string();
        let rootfs_directory = std::path::PathBuf::from(rootfs_directory.as_ref());
        let work_directory = std::path::PathBuf::from(work_directory.as_ref());
        let sandbox_directory = work_directory
            .parent()
            .unwrap()
            .join(format!(".sandbox-{}", sandbox_id));
        log::info!("Init Sandbox");
        // Create && Check Directory

        // Check swapaccount
        if std::path::Path::new("/sys/fs/cgroup/memory/memory.memsw.usage_in_bytes").exists()
            == false
        {
            log::error!("Need \"cgroup_enable=memory swapaccount=1\" kernel parameter");
            return Err(String::from("Didn't have sawpaccount!").into());
        }

        // Check
        if rootfs_directory.exists() == false {
            log::error!("Rootfs Directory isn't exist!");
            return Err(String::from("Rootfs Directory isn't exists!").into());
        }
        if work_directory.exists() == false {
            log::error!("Work Directory isn't exist!");
            return Err(String::from("Work Directory isn't exists!").into());
        }
        if sandbox_directory.exists() == true {
            log::error!("Sandbox Directory is exists");
            return Err(String::from("Sandbox Directory is exists!").into());
        }

        // Create
        std::fs::create_dir(&sandbox_directory)?;
        if sandbox_directory.exists() == false {
            log::error!("Sandbox Directory is exists");
            return Err(String::from("Sandbox Directory is exists!").into());
        }

        // Mount Directory
        let lower_dirs = [&rootfs_directory];
        libmount::Overlay::writable(
            lower_dirs.iter().map(|x| x.as_ref()),
            &work_directory,
            &sandbox_directory,
            &sandbox_directory,
        )
        .mount()?;
        log::info!("Done!");

        // Create new cgroup
        log::debug!("New cgroup {} create", sandbox_id);
        let cur_cgroup = cgroups_fs::CgroupName::new(&sandbox_id);
        let cur_cgroup = cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "memory")?;

        Ok(Sandbox {
            sandbox_id,
            cur_cgroup,
            rootfs_directory,
            sandbox_directory,
            work_directory,
        })
    } //}}}
    pub fn exec(&self, config: SandboxConfig) -> Result<SandboxStatus, Box<dyn Error>> {
        //{{{
        use cgroups_fs::CgroupsCommandExt;
        use wait_timeout::ChildExt;
        let time_limit = std::time::Duration::from_millis(config.time_limit + 500);
        let mut status = SandboxStatusKind::Success;

        // Pre
        log::debug!("Memory Limit {}", config.memory_limit * 2);
        self.cur_cgroup
            .set_value("memory.limit_in_bytes", config.memory_limit * 2)?;
        self.cur_cgroup
            .set_value("memory.memsw.limit_in_bytes", config.memory_limit * 2)?;

        log::info!(
            "Chroot {:?} to run '{}'",
            self.sandbox_directory,
            config.command
        );
        let mut child_exec = std::process::Command::new("bash")
            .args(&["-c", &config.command])
            .current_dir(&self.sandbox_directory)
            .cgroups(&[&self.cur_cgroup])
            .chroot(self.sandbox_directory.to_str().unwrap().to_string())
            .stdin(config.stdin)
            .stdout(config.stdout)
            .stderr(config.stderr)
            .spawn()?;

        log::debug!("spawned");
        // Run command
        let time_start = std::time::Instant::now();
        let return_code = match child_exec.wait_timeout(time_limit)? {
            Some(status) => status.code(),
            _ => {
                log::debug!("Time Limit: {:?}, TLE", time_limit);
                child_exec.kill()?;
                child_exec.wait()?.code()
            }
        };
        let time_end = std::time::Instant::now();

        log::debug!("Return code: {:?}", return_code);
        log::debug!(
            "Start at {:?}, End at {:?}, Use at {:?}",
            time_start,
            time_end,
            time_end - time_start
        );

        // Get return code
        let return_code = match return_code {
            // Rust Crashes
            Some(101) => {
                log::error!("Failed to run command");
                return Err(String::from("Failed to run command").into());
            }
            Some(code) => code,
            None => -1,
        };
        if return_code != 0 {
            status = SandboxStatusKind::RuntimeError;
        }

        // Calc Memory
        let max_memory = self
            .cur_cgroup
            .get_value::<u64>("memory.memsw.max_usage_in_bytes")?;
        if max_memory > config.memory_limit {
            status = SandboxStatusKind::MemoryLimitExceeded;
        }

        // Calc time
        let used_time = time_end - time_start;
        log::debug!("{:?}", used_time);
        if used_time > std::time::Duration::from_millis(config.time_limit) {
            status = SandboxStatusKind::TimeLimitExceeded;
        }
        let used_time = used_time.as_millis();

        Ok(SandboxStatus {
            status,
            max_memory,
            used_time,
            return_code,
        })
    } //}}}
    fn remove(&self) {
        log::info!("Remove Sandbox");
        // Umount rootfs
        let handle_err = |err| {
            log::error!("Failed to umount sandbox: {}", err);
        };

        nix::mount::umount(OsStr::new(&self.sandbox_directory)).unwrap_or_else(handle_err);

        // Remove Directory
        //let handle_err = |err| {
        //    log::error!("Failed to remove sandbox: {}", err);
        //};
        //std::fs::remove_dir_all(&self.sandbox_directory).unwrap_or_else(handle_err);
        log::info!("Done!");
    }
} //}}}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.remove();
    }
}

pub trait SandboxCommandExt {
    fn chroot(&mut self, dir: String) -> &mut Self;
    fn chdir(&mut self, dir: String) -> &mut Self;
}

impl SandboxCommandExt for std::process::Command {
    /// 用于 Command 执行前 Chroot 进入沙箱  
    /// 应该在所有需要修改/读取 sysfs/procfs 的函数之后使用
    fn chroot(&mut self, dir: String) -> &mut Self {
        log::debug!("Chroot to {}", dir);
        unsafe {
            self.pre_exec(move || {
                nix::unistd::chroot(OsStr::new(&dir)).unwrap();
                Ok(())
            })
        }
    }
    /// 用于在 Chroot 之后确定目录  
    /// 应在 `SandboxCommandExt::chroot()` 后使用
    fn chdir(&mut self, dir: String) -> &mut Self {
        unsafe {
            self.pre_exec(move || {
                nix::unistd::chdir(OsStr::new(&dir)).unwrap();
                Ok(())
            })
        }
    }
}
