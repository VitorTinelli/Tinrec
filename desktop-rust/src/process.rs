//! Bounded subprocesses for device discovery and encoder capability probes.
use anyhow::{Context, Result, bail};
use std::{
    io::Read,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn output(command: &mut Command, timeout: Duration) -> Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Não foi possível iniciar a ferramenta de mídia")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.take(4 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = out
        .join()
        .map_err(|_| anyhow::anyhow!("Falha ao ler a ferramenta de mídia"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("Falha ao ler a ferramenta de mídia"))??;
    let Some(status) = status else {
        bail!(
            "A ferramenta de mídia excedeu o tempo limite de {} segundos",
            timeout.as_secs()
        );
    };
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires FFmpeg"]
    fn hung_media_tool_is_killed_and_both_pipes_are_drained() {
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=1",
            "-t",
            "10",
            "-f",
            "null",
            "-",
        ]);
        let start = Instant::now();
        assert!(
            output(&mut command, Duration::from_millis(100))
                .unwrap_err()
                .to_string()
                .contains("tempo limite")
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
