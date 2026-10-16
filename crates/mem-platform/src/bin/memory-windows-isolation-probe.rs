// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit synthetic native acceptance helper; never a release executable.
#[cfg(not(windows))]
fn main() {
    eprintln!("native Windows acceptance only");
    std::process::exit(1);
}

#[cfg(windows)]
mod native {
    use anyhow::{Context, Result, ensure};
    use mem_platform::{
        private_windows::{create_private_directory, create_private_file},
        windows_process::{CodeFile, ProcessSpec, spawn},
    };
    use sha2::{Digest, Sha256};
    use std::{
        fs::{File, OpenOptions},
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        os::windows::io::{AsHandle, AsRawHandle, FromRawHandle},
        path::{Path, PathBuf},
        process::Command,
        ptr::null_mut,
        sync::OnceLock,
        time::{Duration, Instant},
    };

    // Trusted host diagnostics only. Child stderr remains assertion evidence,
    // and no payload, paths, arguments or inherited environment are logged.
    fn stage(name: &'static str) {
        static START: OnceLock<Instant> = OnceLock::new();
        eprintln!(
            "native_probe stage={name} elapsed_ms={}",
            START.get_or_init(Instant::now).elapsed().as_millis()
        );
        let _ = std::io::stderr().flush();
    }
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES, SetFileSecurityW,
        },
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        System::Pipes::CreatePipe,
    };

    fn pipe() -> Result<(File, File)> {
        let mut reader = null_mut();
        let mut writer = null_mut();
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 0,
        };
        ensure!(
            unsafe { CreatePipe(&mut reader, &mut writer, &attrs, 4096) } != 0,
            "create synthetic broker pipe"
        );
        Ok(unsafe { (File::from_raw_handle(reader), File::from_raw_handle(writer)) })
    }
    fn digest(path: &Path) -> Result<[u8; 32]> {
        Ok(Sha256::digest(std::fs::read(path)?).into())
    }
    fn child(args: &[String]) -> Result<()> {
        ensure!(args.len() == 5, "invalid fixture child args");
        let scratch = PathBuf::from(&args[0]);
        let reviewed = PathBuf::from(&args[1]);
        let forbidden = PathBuf::from(&args[2]);
        let address = args[3].parse()?;
        let extra: Vec<_> = args[4].split(':').collect();
        ensure!(extra.len() == 3, "invalid handle fixture");
        let extra_handle = extra[0].parse::<usize>()? as HANDLE;
        let extra_volume = extra[1].parse::<u32>()?;
        let extra_index = extra[2].parse::<u64>()?;
        let mut broker = unsafe {
            File::from_raw_handle(std::env::var("MEMORY_BROKER_HANDLE")?.parse::<usize>()? as HANDLE)
        };
        let mut greeting = [0u8; 12];
        broker.read_exact(&mut greeting)?;
        ensure!(
            &greeting == b"broker-hello",
            "explicit broker pipe inaccessible"
        );
        ensure!(
            std::env::var_os("OPENAI_API_KEY").is_none(),
            "provider environment leaked"
        );
        ensure!(
            File::open(forbidden).is_err(),
            "unrelated private file readable"
        );
        ensure!(
            OpenOptions::new().write(true).open(reviewed).is_err(),
            "code bundle writable"
        );
        std::fs::write(scratch.join("allowed.txt"), b"synthetic scratch only")?;
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        let readable = unsafe { GetFileInformationByHandle(extra_handle, &mut information) } != 0;
        let index =
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
        ensure!(
            !readable || information.dwVolumeSerialNumber != extra_volume || index != extra_index,
            "unlisted inheritable handle leaked"
        );
        let unreviewed = scratch.join("unreviewed.exe");
        // Create a new scratch file rather than CopyFile, which may copy source
        // security metadata and would test a different inheritance path.
        std::fs::write(&unreviewed, std::fs::read(std::env::current_exe()?)?)?;
        // A creator may have implicit WRITE_DAC. Attempt to grant package/owner
        // full control before execution: inherited non-execute ACEs alone are
        // not evidence that an adversarial process cannot enable execution.
        let sddl: Vec<u16> = "D:P(A;;FA;;;AC)(A;;FA;;;OW)"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    null_mut(),
                )
            } != 0,
            "prepare synthetic adversarial ACL"
        );
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<u16> = unreviewed
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let _changed =
            unsafe { SetFileSecurityW(path.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) };
        unsafe {
            LocalFree(descriptor);
        }
        ensure!(
            Command::new(&unreviewed)
                .arg("--unexpected-third")
                .spawn()
                .is_err(),
            "scratch executable was allowed"
        );
        ensure!(
            TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_err(),
            "loopback network access was allowed"
        );
        let status = Command::new(std::env::current_exe()?)
            .arg("--descendant")
            .status()?;
        ensure!(status.success(), "job descendant process limit failed");
        println!(
            "{{\"broker\":true,\"scratch\":true,\"private_file_denied\":true,\"code_write_denied\":true,\"scratch_execute_denied\":true,\"extra_handle_denied\":true,\"loopback_denied\":true,\"third_process_denied\":true}}"
        );
        Ok(())
    }
    pub fn run() -> Result<()> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|v| {
            matches!(
                v.as_str(),
                "--service-layout-child" | "--operator-service-read"
            )
        }) {
            let operator = args[0] == "--operator-service-read";
            ensure!(
                args.len() == if operator { 4 } else { 3 },
                "invalid synthetic service fixture arguments"
            );
            let root = PathBuf::from(&args[1]);
            ensure!(
                root.is_absolute()
                    && root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("MemoryLayer-layout-test-")),
                "synthetic service fixture root required"
            );
            let index: usize = args[2].parse()?;
            ensure!(index < 2, "synthetic fixture index required");
            let own = root.join(format!("configurations/{index}/synthetic.txt"));
            use mem_platform::private_windows::{Input, ServiceInput, ServiceReader};
            if operator {
                let reader = ServiceReader::for_named_service(&args[3])
                    .context("resolve synthetic SCM reader")?;
                ensure!(
                    Input::open(&own, 9).is_err(),
                    "ordinary operator reader unexpectedly trusted service ACE"
                );
                ensure!(
                    ServiceInput::open(&own, 9, &reader)
                        .context("open synthetic service authority")?
                        .read(9)
                        .context("read synthetic service authority")?
                        == b"synthetic",
                    "unexpected synthetic fixture bytes"
                );
                println!("{{\"exact_service_read\":true,\"ordinary_policy_unchanged\":true}}");
            } else {
                ensure!(
                    Input::open(&own, 9)?.read(9)? == b"synthetic",
                    "unexpected synthetic fixture bytes"
                );
                let other = root.join(format!("configurations/{}/synthetic.txt", 1 - index));
                ensure!(
                    Input::open(&other, 9).is_err() && File::open(&other).is_err(),
                    "sibling authority readable"
                );
                ensure!(
                    OpenOptions::new().write(true).open(&own).is_err(),
                    "service authority writable"
                );
                let result = root.join(format!("instances/{index}/result.txt"));
                create_private_file(&result)?.write_all(
                    b"own=True;siblingDenied=True;immutable=True;protectedAncestors=True",
                )?;
                // Keep this one-shot fixture RUNNING until the operator has
                // observed its result. Otherwise SCM can miss the transient
                // state and Start-Service reports failure despite exit 0.
                let finish = root.join(format!("instances/{index}/finish.txt"));
                let deadline = Instant::now() + Duration::from_secs(60);
                while !finish.exists() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                }
                ensure!(
                    std::fs::read(&finish)
                        .context("operator did not acknowledge fixture readiness")?
                        == b"observed",
                    "invalid synthetic finish acknowledgement"
                );
            }
            return Ok(());
        }
        if args.first().is_some_and(|v| v == "--child") {
            return child(&args[1..]);
        }
        if args.first().is_some_and(|v| v == "--descendant") {
            ensure!(
                Command::new(std::env::current_exe()?)
                    .arg("--unexpected-third")
                    .spawn()
                    .is_err(),
                "third process escaped job limit"
            );
            return Ok(());
        }
        ensure!(
            args.is_empty(),
            "probe accepts no operator paths or credentials"
        );
        stage("host_begin");
        let base =
            std::env::temp_dir().join(format!("memory-native-isolation-{}", uuid::Uuid::new_v4()));
        create_private_directory(&base)?;
        let code = base.join("code");
        let scratch = base.join("scratch");
        let neighbor = base.join("other-area");
        for path in [&code, &scratch, &neighbor] {
            create_private_directory(path)?;
        }
        stage("private_directories_created");
        let executable = code.join("probe.exe");
        let mut target = create_private_file(&executable)?;
        std::io::copy(&mut File::open(std::env::current_exe()?)?, &mut target)?;
        target.sync_all()?;
        drop(target);
        let reviewed = code.join("reviewed.txt");
        create_private_file(&reviewed)?.write_all(b"reviewed synthetic data")?;
        let forbidden = neighbor.join("private.txt");
        create_private_file(&forbidden)?.write_all(b"must never be read by child")?;
        stage("synthetic_files_created");
        let (stdin_child, stdin_parent) = pipe()?;
        let (stdout_parent, stdout_child) = pipe()?;
        let (stderr_parent, stderr_child) = pipe()?;
        let (broker_child, mut broker_parent) = pipe()?;
        broker_parent.write_all(b"broker-hello")?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        // Control demonstrates the sentinel is genuinely reachable outside AC.
        drop(TcpStream::connect(listener.local_addr()?)?);
        stage("pipes_and_network_control_created");
        let extra = create_private_file(&base.join("unlisted.txt"))?;
        // Deliberately mark an unrelated authority handle inheritable; HANDLE_LIST
        // must still exclude it. It never contains a real secret.
        ensure!(
            unsafe {
                windows_sys::Win32::Foundation::SetHandleInformation(extra.as_raw_handle(), 1, 1)
            } != 0,
            "prepare extra handle test"
        );
        let mut extra_info = BY_HANDLE_FILE_INFORMATION::default();
        ensure!(
            unsafe { GetFileInformationByHandle(extra.as_raw_handle(), &mut extra_info) } != 0,
            "inspect synthetic extra file"
        );
        let extra_index =
            (u64::from(extra_info.nFileIndexHigh) << 32) | u64::from(extra_info.nFileIndexLow);
        let arguments = vec![
            "--child".into(),
            scratch.as_os_str().to_owned(),
            reviewed.as_os_str().to_owned(),
            forbidden.as_os_str().to_owned(),
            listener.local_addr()?.to_string().into(),
            format!(
                "{}:{}:{}",
                extra.as_raw_handle() as usize,
                extra_info.dwVolumeSerialNumber,
                extra_index
            )
            .into(),
        ];
        stage("before_spawn");
        let mut process = spawn(ProcessSpec {
            executable: CodeFile {
                sha256: digest(&executable)?,
                path: executable,
            },
            runtime_files: vec![CodeFile {
                sha256: digest(&reviewed)?,
                path: reviewed,
            }],
            scratch: scratch.clone(),
            arguments,
            stdin: stdin_child.as_handle(),
            stdout: stdout_child.as_handle(),
            stderr: stderr_child.as_handle(),
            broker: broker_child.as_handle(),
        })
        .with_context(|| format!("native probe state retained at {}", base.display()))?;
        stage("after_spawn");
        drop(stdin_child);
        drop(stdout_child);
        drop(stderr_child);
        drop(broker_child);
        drop(stdin_parent);
        drop(broker_parent);
        let output = std::thread::spawn(move || {
            let mut bytes = String::new();
            stdout_parent
                .take(16 * 1024)
                .read_to_string(&mut bytes)
                .map(|_| bytes)
        });
        let errors = std::thread::spawn(move || {
            let mut bytes = String::new();
            stderr_parent
                .take(16 * 1024)
                .read_to_string(&mut bytes)
                .map(|_| bytes)
        });
        stage("before_child_wait");
        let exit_code = process.wait(Duration::from_secs(30))?;
        stage("after_child_wait");
        ensure!(
            exit_code == 0,
            "native adversarial child failed; state retained at {}",
            base.display()
        );
        stage("before_process_cleanup");
        drop(process);
        stage("after_process_cleanup");
        stage("before_stdout_join");
        let text = output
            .join()
            .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
        stage("after_stdout_join");
        stage("before_stderr_join");
        let stderr = errors
            .join()
            .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
        stage("after_stderr_join");
        ensure!(
            text.contains("\"loopback_denied\":true") && stderr.is_empty(),
            "missing native denial evidence"
        );
        println!("{}", text.trim());
        // Do not recursively delete state: retain the synthetic evidence bundle
        // for ACL inspection and recovery tests. No credential lives there.
        println!("native probe evidence: {}", base.display());
        stage("host_complete");
        Ok(())
    }
}
#[cfg(windows)]
fn main() {
    if let Err(error) = native::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
