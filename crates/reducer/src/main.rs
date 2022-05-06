use std::env;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::anyhow;
use clap::Parser;
use tap::Tap;
use which::Error;

#[derive(Parser)]
struct Options {
    /// Path to the WGSL shader file to reduce.
    shader: PathBuf,

    /// Path to the JSON metadata file.
    ///
    /// If not set, the program will look for a JSON file with the same name as the shader.
    metadata: Option<PathBuf>,

    /// Address of harness server.
    #[clap(short, long)]
    server: Option<String>,

    /// Skip spawning harness server. If this is used then a server address must also be provided.
    #[clap(long, requires = "server")]
    no_spawn_server: bool,

    /// Disable logging from harness.
    #[clap(short, long)]
    quiet: bool,

    /// Enable debug mode for creduce.
    #[clap(long)]
    debug: bool,
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        if let Err(e) = self.0.kill() {
            eprintln!("failed to kill server: {e}");
        }
    }
}

fn main() -> anyhow::Result<()> {
    let options = Options::parse();

    let shader_path = Path::new(&options.shader);
    if !shader_path.exists() {
        return Err(anyhow!("shader at {shader_path:?} does not exist"));
    }

    let shader_path = shader_path.canonicalize()?;

    let metadata_path = if let Some(metadata_path) = options.metadata {
        metadata_path
    } else {
        shader_path
            .parent()
            .unwrap()
            .join(shader_path.file_stem().unwrap())
            .with_extension("json")
    };

    if !metadata_path.exists() {
        return Err(anyhow!("metadata file at {metadata_path:?} does not exist"));
    }

    let metadata_path = metadata_path.canonicalize()?;

    which("tint")?;
    which("naga")?;

    let script_dir = PathBuf::from(env::var("SCRIPT_DIR")?);
    let project_dir = script_dir.parent().unwrap();
    let bin_dir = project_dir.join("target/release");

    let harness_server_path = env::var("HARNESS_SERVER_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            let harness_bin = "harness-server.exe";
            #[cfg(not(target_os = "windows"))]
            let harness_bin = "harness-server";
            project_dir.join("harness/target/release").join(harness_bin)
        });

    let (handle, address) = if options.no_spawn_server {
        (None, options.server.unwrap())
    } else {
        println!(
            "> spawning harness server ({})",
            harness_server_path.display()
        );

        let mut harness = Command::new(harness_server_path)
            .tap_mut(|cmd| {
                if let Some(address) = options.server.as_deref() {
                    cmd.args(["-a", address]);
                }
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdout = BufReader::new(harness.stdout.take().unwrap()).lines();
        let stderr = BufReader::new(harness.stderr.take().unwrap()).lines();

        println!("> waiting for server to start listening");
        let mut address = None;
        for line in &mut stdout {
            let line = match line {
                Ok(line) => line,
                Err(e) => {
                    eprintln!("failed to read from harness server stdout: {e}");
                    break;
                }
            };

            if !options.quiet {
                println!("{line}");
            }

            if let Some(value) = line.strip_prefix("Server listening at ") {
                address = Some(value.trim().to_owned());
                break;
            }
        }

        let thread = std::thread::spawn(move || {
            let stdout_thread = std::thread::spawn(move || {
                for line in stdout.flatten() {
                    if !options.quiet {
                        println!("{line}");
                    }
                }
            });

            let stderr_thread = std::thread::spawn(move || {
                for line in stderr.flatten() {
                    if !options.quiet {
                        println!("{line}");
                    }
                }
            });

            stdout_thread.join().unwrap();
            stderr_thread.join().unwrap();
        });

        let address = address.ok_or_else(|| anyhow!("failed to read harness server address"))?;

        println!("> detected harness server listening at {address}");

        (Some((Server(harness), thread)), address)
    };

    let status = Command::new("creduce")
        .env("WGSLREDUCE_SHADER_NAME", shader_path.file_name().unwrap())
        .env("WGSLREDUCE_METADATA_PATH", metadata_path)
        .env("WGSLREDUCE_SERVER", address)
        .env("WGSLREDUCE_BIN_PATH", bin_dir)
        .arg(script_dir.join("reduce-miscompilation.sh"))
        .arg(shader_path)
        .arg("--not-c")
        .tap_mut(|cmd| {
            if options.debug {
                cmd.arg("--debug");
            }
        })
        .status()?;

    if let Some((handle, thread)) = handle {
        drop(handle);
        thread.join().unwrap();
    }

    if !status.success() {
        return Err(anyhow!("creduce did not complete successfully"));
    }

    Ok(())
}

fn which(bin: &str) -> anyhow::Result<PathBuf> {
    match which::which(bin) {
        Ok(path) => Ok(path),
        Err(e) => {
            if let Error::CannotFindBinaryPath = e {
                Err(anyhow!("cannot find executable path: {bin}"))
            } else {
                Err(e.into())
            }
        }
    }
}