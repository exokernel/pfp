use log::debug;
use pfp::*;
use signal_hook::consts::{SIGINT, SIGTERM};
use std::error::Error;
use std::path::Path;
use std::process;
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
    input_path: String,
}

/// Do the thing forever unless interrupted.
/// Read all files in the input path and feed them in chunks to an invocation of Gnu Parallel
/// Wait for each chunk to complete before processing the next chunk
/// TODO: Possibly sleep after processing each chunk?
fn run(
    chunk_size: usize,
    job_slots: String,
    sleep_time: u64,
    daemon: bool,
    extensions: Vec<&str>,
    input_path: String,
    script: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGTERM, Arc::clone(&term))?;
    signal_hook::flag::register(SIGINT, Arc::clone(&term))?;

    let slots = job_slots.as_str();

    let mut command = String::from("echo $(date) {#}-{%}-{}");
    if script.is_some() {
        command = script.unwrap();
    }

    // Do forever
    loop {
        print(format!("PFP: LOOP START").as_str());

        if should_term(&term) {
            return Ok(());
        }

        // 1. Get all the files in our input path
        let mut files: Vec<String> = vec![];
        if !files.is_empty() {
            panic!("files is not empty");
        }
        //get_files(Path::new(&input_path), &extensions, &mut files)?;
        get_files2(Path::new(&input_path), &mut files, &mut |path, files| {
            if extensions.is_empty()
                || extensions.contains(&path.extension().unwrap().to_str().unwrap())
            {
                // only add files with the given extensions or all files if none were given
                files.push(path.display().to_string());
            }
        })?;

        if should_term(&term) {
            return Ok(());
        }

        // 2. process chunks of input in parallel
        let num_chunks = files.len() / chunk_size;
        let leftover = files.len() % chunk_size;
        debug!("number of chunks {}", num_chunks);
        debug!("leftover {}", leftover);

        // ??? how to break input up into chunks

        // Each chunk is a reference to slice of our Vec<String>:  &[String] &["foo", "bar",...,"baz"]
        // chunk 1:  [0..50] 0-49 50-things
        // chunk 2:  [50..100] 50-99 50-things
        // chunk 3:  [100..101] 100 1-thing

        let mut total_chunks = num_chunks;
        if leftover != 0 {
            total_chunks += 1;
        }

        for n in 0..num_chunks {
            debug!("chunk {}/{} ({}): START", n + 1, total_chunks, chunk_size);
            debug!(
                "chunk start: {} chunk_end: {}",
                n * chunk_size,
                n * chunk_size + chunk_size - 1
            );
            let chunk = get_chunk(n * chunk_size, chunk_size, &files);
            parallelize(&command, &slots, chunk)?;
            debug!("chunk {}/{} ({}): DONE", n + 1, total_chunks, chunk_size);
            if should_term(&term) {
                return Ok(());
            }
        }

        // last chunk
        if leftover != 0 {
            debug!(
                "chunk {}/{} ({}): START",
                num_chunks + 1,
                total_chunks,
                leftover
            );
            debug!(
                "chunk start: {} chunk_end: {}",
                num_chunks * chunk_size,
                num_chunks * chunk_size + leftover - 1
            );
            let chunk = get_chunk(num_chunks * chunk_size, leftover, &files);
            parallelize(&command, &slots, chunk)?;
            debug!(
                "chunk {}/{} ({}): DONE",
                num_chunks + 1,
                total_chunks,
                leftover
            );
        }

        // 3. Do any necessary postprocessing

        if (!daemon) || should_term(&term) {
            return Ok(());
        }

        print(
            format!(
                "PFP: Finished processing all files in input-path. Sleeping for {} seconds...",
                sleep_time
            )
            .as_str(),
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

    // By default we'll use -j 100% to run one job per CPU core
    let mut job_slots = String::from("100%");
    if opt.job_slots.is_some() {
        job_slots = opt.job_slots.unwrap().to_string();
    }

    let ext_string: String;
    let ext_vec: Vec<&str>;
    if opt.extensions.is_some() {
        ext_string = opt.extensions.clone().unwrap();
        ext_vec = ext_string.as_str().split(",").collect();
    } else {
        ext_vec = vec![];
    }

    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .init();

    debug!("{:?}", opt);
    debug!("job_slots = {}", job_slots);

    if let Err(e) = run(
        opt.chunk_size,
        job_slots,
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
