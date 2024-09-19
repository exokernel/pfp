use anyhow::Result;
use clap::Parser;
use pfp::*;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use std::str::FromStr;

fn parse_duration(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = u64::from_str(arg)?;
    Ok(Duration::from_secs(seconds))
}

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
    #[clap(short = 't', long = "sleep-time", default_value = "5", value_parser = parse_duration)]
    sleep_time: Duration,

    /// Shell script to run in parallel
    #[clap(short, long)]
    script: Option<PathBuf>,

    /// Directory to read files from
    input_path: PathBuf,
}

fn configure_thread_pool(job_slots: Option<usize>) -> Result<()> {
    if let Some(slots) = job_slots {
        rayon::ThreadPoolBuilder::new()
            .num_threads(slots)
            .build_global()?;
    } else {
        rayon::ThreadPoolBuilder::new().build_global()?;
    }
    Ok(())
}

fn process_files(context: &AppContext) -> Result<()> {
    let files = get_files(context)?;

    if context.should_term() {
        log::info!("Received termination signal. Exiting...");
        return Ok(());
    }

    let (processed_files, errored_files) = process_file_chunks(context, &files)?;

    log_processing_results(&files, processed_files, errored_files);

    Ok(())
}

fn process_file_chunks(context: &AppContext, files: &[PathBuf]) -> Result<(usize, usize)> {
    let total_chunks = (files.len() + context.chunk_size() - 1) / context.chunk_size();
    let mut processed_files = 0;
    let mut errored_files = 0;

    for (n, chunk) in files.chunks(context.chunk_size()).enumerate() {
        if context.should_term() {
            log::info!("Received termination signal. Exiting...");
            return Ok((processed_files, errored_files));
        }

        log::debug!("chunk {}/{} ({}): START", n + 1, total_chunks, chunk.len());

        let (processed, errored) = parallelize_chunk(context, chunk)?;

        processed_files += processed;
        errored_files += errored;

        log::debug!("chunk {}/{} ({}): DONE", n + 1, total_chunks, chunk.len());
    }

    Ok((processed_files, errored_files))
}

fn log_processing_results(files: &[PathBuf], processed_files: usize, errored_files: usize) {
    log::debug!("Total number of files {}", files.len());
    log::debug!("Total number of processed files {}", processed_files);
    log::debug!("Total number of errored files {}", errored_files);
    log::info!("PFP: Finished processing all files in input-path.");
}

fn sleep_daemon(sleep_time: Duration) {
    log::info!("Sleeping for {:?}...", sleep_time);
    std::thread::sleep(sleep_time);
}

/// Do the thing forever unless interrupted.
/// Read all files in the input path and break them into chunks to execute in parallel
/// Wait for each chunk to complete before processing the next chunk
fn run(
    context: AppContext,
    job_slots: Option<usize>,
    sleep_time: Duration,
    daemon: bool,
) -> Result<()> {
    configure_thread_pool(job_slots)?;

    loop {
        log::info!("PFP: LOOP START");

        if context.should_term() {
            log::info!("Received termination signal. Exiting...");
            return Ok(());
        }

        process_files(&context)?;

        if !daemon {
            log::info!("Not running in daemon mode. Exiting after processing all files.");
        }

        if context.should_term() {
            log::info!("Received termination signal. Exiting...");
            return Ok(());
        }

        sleep_daemon(sleep_time);
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
                "Script path does not exist: {:?}",
                script_path
            ));
        }
        if !script_path.is_file() {
            return Err(anyhow::anyhow!(
                "Script path is not a file: {:?}",
                script_path
            ));
        }
    }

    // Process the extensions input:
    // 1. Split the comma-separated string into individual extensions
    // 2. Trim whitespace from each extension
    // 3. Remove any empty extensions
    // 4. Convert each extension to an OsString
    // 5. Collect the results into a Vec<OsString>
    // If no extensions were provided, ext_vec will be None
    let extensions = opt.extensions.as_ref().map(|s| {
        s.split(',')
            .map(|ext| OsString::from(ext.trim()))
            .filter(|ext| !ext.is_empty())
            .collect::<Vec<OsString>>()
    });

    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .init();

    log::debug!("{:?}", opt);
    log::debug!("Parsed extensions: {:?}", extensions);
    if let Some(slots) = opt.job_slots {
        log::debug!("job_slots = {}", slots);
    }

    let context = AppContext::new(opt.chunk_size, extensions, opt.input_path, opt.script)?;

    run(context, opt.job_slots, opt.sleep_time, opt.daemon)?;

    Ok(())
}
