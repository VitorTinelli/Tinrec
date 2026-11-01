use tinrec_capture::{RecordOptions, audio, diagnostics, encoder, recording};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

#[derive(Parser)]
#[command(
    version,
    about = "Protótipo Tinrec: captura nativa multiplataforma e diagnóstico, sem upload"
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Examina backends e dependências sem gravar.
    Doctor {
        #[arg(long, default_value = "ffmpeg")]
        ffmpeg: PathBuf,
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,
    },
    /// Lista fontes de tela e microfones; pode solicitar permissão no macOS.
    Sources,
    /// Grava tela e, opcionalmente, áudio do sistema e microfone.
    Record(RecordOptions),
    /// Testa encoder, timestamps e pausas com imagem e tons gerados; não testa hardware.
    Synthetic(RecordOptions),
    /// Conta quadros e mostra metadados de um arquivo existente.
    Inspect {
        file: PathBuf,
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,
    },
}
fn tool(path: &std::path::Path) -> Result<String> {
    let result = Command::new(path).arg("-version").output()?;
    if !result.status.success() {
        bail!("Falha ao executar {}", path.display());
    }
    Ok(String::from_utf8_lossy(&result.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned())
}
fn execute() -> Result<()> {
    let command = Cli::parse().command;
    let synthetic = matches!(&command, Action::Synthetic(_));
    match command {
        Action::Doctor { ffmpeg, ffprobe } => {
            println!(
                "Sistema: {} / {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            println!("{}\n{}", tool(&ffmpeg)?, tool(&ffprobe)?);
            let encoders = Command::new(&ffmpeg)
                .args(["-hide_banner", "-encoders"])
                .output()?;
            if !String::from_utf8_lossy(&encoders.stdout).contains("libx264") {
                bail!("FFmpeg sem encoder libx264");
            }
            for backend in pinray::available_backends() {
                println!("{backend:?}");
            }
            println!(
                "Diagnóstico de dependências concluído; captura e permissões ainda precisam de teste real."
            );
        }
        Action::Sources => {
            for source in pinray::enumerate_sources()? {
                println!("{source:?}");
            }
            for mic in audio::list()? {
                println!("Microfone: {mic}");
            }
        }
        Action::Record(options) | Action::Synthetic(options) => {
            if options.output.extension().and_then(|s| s.to_str()) != Some("mp4") {
                bail!("Use um destino com extensão .mp4");
            }
            for suffix in [
                "",
                ".partial.mp4",
                ".ffmpeg.log",
                ".capture.json",
                ".ffprobe.json",
            ] {
                if encoder::sidecar(&options.output, suffix).exists() {
                    bail!(
                        "Destino já existe: {}{suffix}. Escolha outro nome.",
                        options.output.display()
                    );
                }
            }
            tool(&options.ffmpeg)?;
            tool(&options.ffprobe)?;
            let report = recording::run(&options, synthetic)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.status != "completed" {
                bail!("{}", report.error.as_deref().unwrap_or("Gravação falhou"));
            }
        }
        Action::Inspect { file, ffprobe } => println!(
            "{}",
            serde_json::to_string_pretty(&diagnostics::probe(&ffprobe, &file)?)?
        ),
    }
    Ok(())
}
fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Erro: {error:#}");
            ExitCode::FAILURE
        }
    }
}
