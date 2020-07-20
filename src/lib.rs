use std::error::Error;
use std::os::unix::process::CommandExt;
use std::process::Stdio;

pub struct SandboxConfig {
    pub command: String,
    pub time_limit: u64,
    pub memory_limit: u64,
    pub pids_limit: u16,
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
}

impl SandboxConfig {
    fn gen(
        time_limit: u64,
        memory_limit: u64,
        pids_limit: u16,
        command: String,
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> SandboxConfig {
        SandboxConfig {
            time_limit: time_limit,
            memory_limit: memory_limit,
            pids_limit: pids_limit,
            command: command,
            stdin,
            stdout,
            stderr,
        }
    }
}

struct SandboxCgroup {
    memory: cgroups_fs::AutomanagedCgroup,
    pids: cgroups_fs::AutomanagedCgroup,
}

impl SandboxCgroup {
    fn create(cgroup_name: &str) -> Result<SandboxCgroup, Box<dyn Error>> {
        let cur_cgroup = cgroups_fs::CgroupName::new(cgroup_name);
        Ok(SandboxCgroup {
            memory: cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "memory")?,
            pids: cgroups_fs::AutomanagedCgroup::init(&cur_cgroup, "pids")?,
        })
    }
}

pub struct Sandbox {
    pub sandbox_directory: std::path::PathBuf,
    work_directory: std::path::PathBuf,
    rootfs_directory: std::path::PathBuf,
    mounted: bool,
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

impl Sandbox {
    pub fn create<T, U, V>(
        rootfs_directory: T,
        work_directory: U,
        sandbox_directory: V,
    ) -> Result<Sandbox, Box<dyn Error>>
    where
        T: AsRef<std::path::Path>,
        U: AsRef<std::path::Path>,
        V: AsRef<std::path::Path>,
    {
        let sandbox_id = uuid::Uuid::new_v4().to_string();
        let rootfs_directory = std::path::PathBuf::from(rootfs_directory.as_ref());
        let work_directory = std::path::PathBuf::from(work_directory.as_ref());
        let sandbox_directory = std::path::PathBuf::from(sandbox_directory.as_ref());

        let log_and_panic = |err: &str| -> Result<(), Box<dyn Error>> {
            log::error!("{}", err);
            return Err(String::from(err).into());
        };

        let check_directory = |directory: &std::path::PathBuf| -> Result<(), Box<dyn Error>> {
            if directory.exists() == false {
                log_and_panic(&format!("{:?} Not Found!", directory))?;
            }
            Ok(())
        };

        // Check swapaccount
        if std::path::Path::new("/sys/fs/cgroup/memory/memory.memsw.usage_in_bytes").exists()
            == false
        {
            log_and_panic("Need \"cgroup_enable=memory swapaccount=1\" kernel parameter")?;
        }

        check_directory(&rootfs_directory)?;
        check_directory(&work_directory)?;
        check_directory(&sandbox_directory)?;

        // Mount Directory
        let lower_dirs = [&rootfs_directory];
        libmount::Overlay::writable(
            lower_dirs.iter().map(|x| x.as_ref()),
            &work_directory,
            &sandbox_directory,
            &sandbox_directory,
        )
        .mount()?;

        Ok(Sandbox {
            sandbox_directory,
            work_directory,
            rootfs_directory,
            mounted: true,
        })
    }
    pub fn run(&self, config: SandboxConfig) -> Result<SandboxStatus, Box<dyn Error>> {
        use cgroups_fs::CgroupsCommandExt;
        use wait_timeout::ChildExt;

        // Set cgroup limit
        let cgroup = SandboxCgroup::create(&uuid::Uuid::new_v4().to_string()).unwrap();
        let time_limit = std::time::Duration::from_millis(config.time_limit + 500);
        let mut status = SandboxStatusKind::Success;
        cgroup
            .memory
            .set_value("memory.limit_in_bytes", config.memory_limit * 2)
            .unwrap();
        cgroup
            .memory
            .set_value("memory.memsw.limit_in_bytes", config.memory_limit * 2)
            .unwrap();
        cgroup
            .pids
            .set_value("pids.max", config.pids_limit)
            .unwrap();

        // Create Child
        let mut child_exec = std::process::Command::new("bash")
            .args(&["-c", &config.command])
            .current_dir(&self.sandbox_directory)
            .cgroups(&[&cgroup.memory, &cgroup.pids])
            .chroot(self.sandbox_directory.to_str().unwrap().to_string())
            .setns(nix::sched::CloneFlags::CLONE_NEWPID)
            .stdin(config.stdin)
            .stdout(config.stdout)
            .stderr(config.stderr)
            .spawn()
            .unwrap();

        let time_start = std::time::Instant::now();
        let return_code = match child_exec.wait_timeout(time_limit).unwrap() {
            Some(status) => status.code(),
            _ => {
                log::debug!("Time Limit: {:?}, TLE", time_limit);
                child_exec.kill().unwrap();
                child_exec.wait().unwrap().code()
            }
        };
        let time_end = std::time::Instant::now();

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
        let max_memory = cgroup
            .memory
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
    }
}

pub trait SandboxCommandExt {
    fn chroot(&mut self, dir: String) -> &mut Self;
    fn chdir(&mut self, dir: String) -> &mut Self;
    fn setns(&mut self, nsflag: nix::sched::CloneFlags) -> &mut Self;
}

impl SandboxCommandExt for std::process::Command {
    /// 用于 Command 执行前 Chroot 进入沙箱  
    /// 应该在所有需要修改/读取 sysfs/procfs 的函数之后使用
    fn chroot(&mut self, dir: String) -> &mut Self {
        use std::ffi::OsStr;
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
        use std::ffi::OsStr;
        log::debug!("Chroot to {}", dir);
        unsafe {
            self.pre_exec(move || {
                nix::unistd::chdir(OsStr::new(&dir)).unwrap();
                Ok(())
            })
        }
    }
    fn setns(&mut self, nsflags: nix::sched::CloneFlags) -> &mut Self {
        unsafe {
            self.pre_exec(move || {
                nix::sched::setns(std::process::id() as i32, nsflags).unwrap();
                Ok(())
            })
        }
    }
}
