use anyhow::{anyhow, Result};
use clap::Parser;
use pfp::ProcessingContext;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use pfp::term_if_signal_rcvd;

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

fn process_files(context: &ProcessingContext) -> Result<()> {
    let files = pfp::get_files(context.input_path, context.extensions)?;

    term_if_signal_rcvd!(context);

    let (processed_files, errored_files, cancelled_files) = process_file_chunks(context, &files)?;

    log_processing_results(&files, processed_files, errored_files, cancelled_files);

    Ok(())
}

fn process_file_chunks(
    context: &ProcessingContext,
    files: &[PathBuf],
) -> Result<(usize, usize, usize)> {
    let total_chunks = (files.len() + context.chunk_size - 1) / context.chunk_size;
    let mut processed_files = 0;
    let mut errored_files = 0;
    let mut cancelled_files = 0;

    for (n, chunk) in files.chunks(context.chunk_size).enumerate() {
        term_if_signal_rcvd!(context, (processed_files, errored_files, cancelled_files));

        log::debug!("chunk {}/{} ({}): START", n + 1, total_chunks, chunk.len());

        let should_cancel = || context.term_signal_rcvd();
        let (processed, errored, cancelled) =
            pfp::parallelize_chunk(chunk, context.script, should_cancel)?;

        processed_files += processed;
        errored_files += errored;
        cancelled_files += cancelled;

        log::debug!("chunk {}/{} ({}): DONE", n + 1, total_chunks, chunk.len());
    }

    Ok((processed_files, errored_files, cancelled_files))
}

fn log_processing_results(
    files: &[PathBuf],
    processed_files: usize,
    errored_files: usize,
    cancelled_files: usize,
) {
    log::debug!("Total number of files {}", files.len());
    log::debug!("Total number of processed files {}", processed_files);
    log::debug!("Total number of errored files {}", errored_files);
    log::debug!("Total number of cancelled files {}", cancelled_files);
    log::info!("PFP: Finished processing all files in input-path.");
}

fn sleep_daemon(sleep_time: u64) {
    log::info!("Sleeping for {} seconds...", sleep_time);
    sleep(Duration::from_secs(sleep_time));
}

/// Do the thing forever unless interrupted.
/// Read all files in the input path and break them into chunks to execute in parallel
/// Wait for each chunk to complete before processing the next chunk
fn run(context: &ProcessingContext) -> Result<()> {
    context.setup_signal_handling()?;
    context.configure_thread_pool()?;

    loop {
        log::info!("PFP: LOOP START");

        term_if_signal_rcvd!(context);

        process_files(context)?;

        if !context.daemon {
            log::info!("PFP: Not running as daemon, exiting...");
            return Ok(());
        }

        term_if_signal_rcvd!(context);

        sleep_daemon(context.sleep_time);
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
            return Err(anyhow!("Script path does not exist: {:?}", script_path));
        }
        if !script_path.is_file() {
            return Err(anyhow!("Script path is not a file: {:?}", script_path));
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

    log::debug!("Options: {:?}", opt);
    log::debug!("Parsed extensions: {:?}", ext_vec);

    let context = ProcessingContext {
        chunk_size: opt.chunk_size,
        extensions: &ext_vec,
        input_path: &opt.input_path,
        job_slots: opt.job_slots,
        script: opt.script.as_deref(),
        term: Arc::new(AtomicBool::new(false)),
        sleep_time: opt.sleep_time,
        daemon: opt.daemon,
    };

    run(&context)?;

    Ok(())
}
