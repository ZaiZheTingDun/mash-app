//! Host child-process policy, shared by ADB callers.

use std::io::{self, Read, Seek, SeekFrom};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub(crate) fn external_command(path: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

// Capture into temporary files instead of pipes: a daemon inheriting stdout
// must not keep the caller waiting for EOF after the direct child exits.
pub(crate) fn output_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()?;
    let status = wait_with_timeout(&mut child, timeout)?;
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut output = Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let stdout_len = stdout.metadata()?.len();
    let stderr_len = stderr.metadata()?.len();
    stdout.take(stdout_len).read_to_end(&mut output.stdout)?;
    stderr.take(stderr_len).read_to_end(&mut output.stderr)?;
    Ok(output)
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> io::Result<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(
                    Duration::from_millis(10).min(timeout.saturating_sub(started.elapsed())),
                );
            }
            result => {
                // Always reap the child, even if checking its status failed.
                // It may have exited between try_wait and kill.
                let _ = child.kill();
                child.wait()?;
                return match result {
                    Err(error) => Err(error),
                    _ => Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "external command timed out",
                    )),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sleeping_command() -> Command {
        #[cfg(windows)]
        {
            let mut command = external_command("powershell.exe");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ]);
            command
        }
        #[cfg(not(windows))]
        {
            let mut command = external_command("sleep");
            command.arg("30");
            command
        }
    }

    #[test]
    fn timeout_kills_and_reaps_the_child() {
        let mut child = sleeping_command().spawn().unwrap();
        let started = Instant::now();
        let error = wait_with_timeout(&mut child, Duration::from_millis(100)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!child.try_wait().unwrap().unwrap().success());
    }

    #[test]
    fn timed_out_output_returns_and_next_command_can_complete() {
        assert_eq!(
            output_with_timeout(&mut sleeping_command(), Duration::from_millis(100))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        #[cfg(windows)]
        let mut command = {
            let mut command = external_command("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-Command",
                "[Console]::Out.Write(('o' * 200000)); [Console]::Error.Write(('e' * 200000)); exit 7"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = external_command("sh");
            command.args(["-c", "awk 'BEGIN {for (i=0;i<200000;i++) printf \"o\"}'; awk 'BEGIN {for (i=0;i<200000;i++) printf \"e\"}' >&2; exit 7"]);
            command
        };
        let output = output_with_timeout(&mut command, Duration::from_secs(10)).unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, vec![b'o'; 200000]);
        assert_eq!(output.stderr, vec![b'e'; 200000]);
    }
}
