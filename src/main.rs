use log::{debug, error};
use pfp::*;
use rayon::prelude::*;
use signal_hook::consts::{SIGINT, SIGTERM};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "pfp", about = "Parallel File Processor")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Process files in input path continuously
    #[structopt(long)]
    daemon: bool,

    /// List of extensions delimited by commas. Only files ending in these extensions
    /// will be processed. E.g. -e "mp4,flv"
    /// If this option is not provided then all files under the input_path will be processed
    #[structopt(short, long)]
    extensions: Option<String>,

    /// Number of things to try to do in parallel at one time.
    /// This is the number inputs that will be fed to a single invocation of
    /// Gnu Parallel. The actual number of parallel jobs per chunk is limited
    /// by job_slots.
    #[structopt(short, long, default_value = "50")]
    chunk_size: usize,

    /// Number of parallel job slots to use. Default to 1 slot per CPU core.
    #[structopt(short, long)]
    job_slots: Option<usize>,

    /// Seconds to sleep before processing all files in input_path again.
    /// Only used if --daemon is specified
    #[structopt(short = "t", long = "sleep-time", default_value = "5")]
    sleep_time: u64,

    /// Shell command or script to run in parallel
    #[structopt(short, long)]
    script: Option<String>,

    /// Directory to read files from
    input_path: PathBuf,
}

/// Do the thing forever unless interrupted.
/// Read all files in the input path and feed them in chunks to rayon to execute in parallel
/// Wait for each chunk to complete before processing the next chunk
fn run(
    chunk_size: usize,
    job_slots: Option<usize>,
    sleep_time: u64,
    daemon: bool,
    extensions: Option<Vec<&str>>,
    input_path: PathBuf,
    script: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGTERM, Arc::clone(&term))?;
    signal_hook::flag::register(SIGINT, Arc::clone(&term))?;

    let mut command = String::from("echo");
    if script.is_some() {
        command = script.unwrap();
    }

    // Do forever
    loop {
        print("PFP: LOOP START");

        if should_term(&term) {
            return Ok(());
        }

        // 1. Get all the files in our input path
        let mut files: Vec<String> = vec![];
        if !files.is_empty() {
            panic!("files is not empty");
        }
        //get_files(Path::new(&input_path), &extensions, &mut files)?;
        get_files2(Path::new(&input_path), &mut |path| {
            if extensions.as_ref().is_none()
                || (path.extension().is_some()
                    && extensions
                        .as_ref()
                        .unwrap()
                        .contains(&path.extension().unwrap().to_str().unwrap()))
            {
                // only add files with the given extensions or all files if none were given
                files.push(path.display().to_string());
            }
        })?;

        if should_term(&term) {
            return Ok(());
        }

        // 2. process chunks of input in parallel
        let total_chunks = (files.len() + chunk_size - 1) / chunk_size; // Ceiling division

        // Configure the thread pool
        // In your run function:
        if let Some(slots) = job_slots {
            // If job_slots is specified, use that number
            rayon::ThreadPoolBuilder::new()
                .num_threads(slots)
                .build_global()?;
        } else {
            // If job_slots is not specified, let Rayon use its default
            rayon::ThreadPoolBuilder::new().build_global()?;
        }

        let mut processed_chunks = 0;

        debug!("command: {}", command);

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

            chunk
                .par_iter()
                .try_for_each(|file| -> Result<(), Box<dyn Error + Send + Sync>> {
                    let output = Command::new(&command).arg(file).output()?;

                    if !output.status.success() {
                        error!("Command failed for file: {}", file);
                    } else {
                        debug!("Processed file: {}", file);
                        debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
                    }

                    Ok(())
                })?;

            processed_chunks += 1;

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

        // 3. Do any necessary postprocessing

        if !daemon || should_term(&term) {
            return Ok(());
        }

        log::info!(
            "PFP: Finished processing all files in input-path. Sleeping for {} seconds...",
            sleep_time
        );
        std::thread::sleep(std::time::Duration::from_secs(sleep_time));
    }
}

/// Parse cli args and then do the thing
fn main() {
    let opt = Opt::from_args();
    if opt.debug {
        std::env::set_var("RUST_LOG", "debug");
    }

    let ext_vec = opt.extensions.as_ref().map(|s| s.split(",").collect());

    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .init();

    debug!("{:?}", opt);
    if opt.job_slots.is_some() {
        debug!("job_slots = {}", opt.job_slots.unwrap());
    }

    if let Err(e) = run(
        opt.chunk_size,
        opt.job_slots,
        opt.sleep_time,
        opt.daemon,
        ext_vec,
        opt.input_path,
        opt.script,
    ) {
        eprint(format!("Oh noes! {}", e).as_str());
        process::exit(1);
    }
}
