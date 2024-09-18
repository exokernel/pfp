use anyhow::{Context, Result};
use clap::Parser;
use log::debug;
use pfp::*;
use signal_hook::consts::{SIGINT, SIGTERM};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[clap(name = "pfp", about = "Parallel File Processor")]
struct Opt {
    /// Activate debug mode
    #[clap(short, long)]
    debug: bool,

    /// Process files in input path continuously
    #[clap(long)]
    daemon: bool,

    /// List of extensions delimited by commas. Only files ending in these extensions
    /// will be processed. E.g. -e "mp4,flv"
    /// If this option is not provided then all files under the input_path will be processed
    #[clap(short, long)]
    extensions: Option<String>,

    /// Number of things to try to do in parallel at one time.
    /// This is the number inputs that will be fed to Rayon. The actual number of parallel jobs per chunk is limited
    /// by job_slots.
    #[clap(short, long, default_value = "50")]
    chunk_size: usize,

    /// Number of parallel job slots to use. Default to 1 slot per CPU core.
    #[clap(short, long)]
    job_slots: Option<usize>,

    /// Seconds to sleep before processing all files in input_path again.
    /// Only used if --daemon is specified
    #[clap(short = 't', long = "sleep-time", default_value = "5")]
    sleep_time: u64,

    /// Shell script to run in parallel
    #[clap(short, long)]
    script: Option<PathBuf>,

    /// Directory to read files from
    input_path: PathBuf,
}

/// Do the thing forever unless interrupted.
/// Read all files in the input path and break them into chunks to execute in parallel
/// Wait for each chunk to complete before processing the next chunk
fn run(
    chunk_size: usize,
    job_slots: Option<usize>,
    sleep_time: u64,
    daemon: bool,
    extensions: Option<Vec<&OsStr>>,
    input_path: &Path,
    script: Option<&Path>,
) -> Result<()> {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGTERM, term.clone())?;
    signal_hook::flag::register(SIGINT, term.clone())?;

    // Configure the thread pool
    if let Some(slots) = job_slots {
        // If job_slots is specified, use that number
        rayon::ThreadPoolBuilder::new()
            .num_threads(slots)
            .build_global()?;
    } else {
        // If job_slots is not specified, let Rayon use its default
        rayon::ThreadPoolBuilder::new().build_global()?;
    }

    // Do forever
    loop {
        log::info!("PFP: LOOP START");

        if should_term(&term) {
            return Ok(());
        }

        // 1. Get all the files in our input path
        let files: Vec<PathBuf> = get_files(input_path, &extensions)?;

        if should_term(&term) {
            return Ok(());
        }

        // 2. process chunks of input in parallel
        let total_chunks = (files.len() + chunk_size - 1) / chunk_size; // Ceiling division

        let mut processed_chunks = 0;
        let mut processed_files = 0;
        let mut errored_files = 0;

        if script.is_some() {
            log::debug!("Using script: {:?}", script);
        }

        for (n, chunk) in files.chunks(chunk_size).enumerate() {
            if should_term(&term) {
                return Ok(());
            }

            debug!("chunk {}/{} ({}): START", n + 1, total_chunks, chunk.len());
            debug!(
                "chunk start: {} chunk_end: {}",
                n * chunk_size,
                n * chunk_size + chunk.len() - 1
            );

            let (processed, errored) = parallelize_chunk(chunk, script, &term)?;

            processed_chunks += 1;
            processed_files += processed;
            errored_files += errored;

            debug!(
                "chunk {}/{} ({}): DONE",
                processed_chunks,
                total_chunks,
                chunk.len()
            );
        }

        debug!(
            "Processed {} out of {} chunks",
            processed_chunks, total_chunks
        );
        debug!("Total number of files {}", files.len());
        debug!("Total number of processed files {}", processed_files);
        debug!("Total number of errored files {}", errored_files);

        // 3. Do any necessary postprocessing
        log::info!("PFP: Finished processing all files in input-path.");

        if !daemon || should_term(&term) {
            return Ok(());
        }

        log::info!("Sleeping for {} seconds...", sleep_time);
        std::thread::sleep(std::time::Duration::from_secs(sleep_time));
    }
}

/// Parse cli args and then do the thing
fn main() -> Result<()> {
    let opt = Opt::parse();
    if opt.debug {
        std::env::set_var("RUST_LOG", "debug");
    }

    // Validate script if provided
    if let Some(script_path) = &opt.script {
        if !script_path.exists() {
            return Err(anyhow::anyhow!(
                "Script file does not exist: {:?}",
                script_path
            ));
        }

        // Unix-specific permission checks
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(script_path)
                .with_context(|| format!("Failed to get metadata for script: {:?}", script_path))?;
            let permissions = metadata.permissions();

            // Check if the script is readable
            if permissions.mode() & 0o444 == 0 {
                return Err(anyhow::anyhow!(
                    "Script file is not readable: {:?}",
                    script_path
                ));
            }

            // Check if the script is executable
            if permissions.mode() & 0o111 == 0 {
                return Err(anyhow::anyhow!(
                    "Script file is not executable: {:?}",
                    script_path
                ));
            }
        }
    }

    // Process the extensions input:
    // 1. Split the comma-separated string into individual extensions
    // 2. Trim whitespace from each extension
    // 3. Remove any empty extensions
    // 4. Convert each extension to an OsStr
    // 5. Collect the results into a Vec<&OsStr>
    // If no extensions were provided, ext_vec will be None
    let ext_vec = opt.extensions.as_ref().map(|s| {
        s.split(",")
            .map(|ext| ext.trim())
            .filter(|ext| !ext.is_empty())
            .map(OsStr::new)
            .collect::<Vec<&OsStr>>()
    });

    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .init();

    debug!("{:?}", opt);
    debug!("Parsed extensions: {:?}", ext_vec);
    if let Some(slots) = opt.job_slots {
        debug!("job_slots = {}", slots);
    }

    run(
        opt.chunk_size,
        opt.job_slots,
        opt.sleep_time,
        opt.daemon,
        ext_vec,
        &opt.input_path,
        opt.script.as_deref(),
    )?;

    Ok(())
}
