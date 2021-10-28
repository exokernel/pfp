use std::io::{Write};
use std::io;
use std::process::{Command, Stdio};
use std::process;
use std::error::Error;
use structopt::StructOpt;
use log::{debug};
use std::fs;
use std::path::{Path};
use chrono::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
//use signal_hook;


#[derive(Debug, StructOpt)]
#[structopt(name = "pfp", about = "Parallel File Processor")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    // Activate test mode
    //#[structopt(short, long)]
    //test: bool,

    /// Process files in input path continuously
    #[structopt(long)]
    daemon: bool,

    /// List of extensions delimited by commas. Only files ending in these extensions
    /// will be processed. E.g. -e "mp4,flv"
    /// If this option is not provided then all files under the input_path will be processed
    #[structopt(short,long)]
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
    #[structopt(short="t", long="sleep-time", default_value="5")]
    sleep_time: u64,

    /// Shell command or script to run in parallel
    #[structopt(short, long)]
    script: Option<String>,

    /// Directory to read files from
    input_path: String,
}

enum Fd {
    StdOut,
    StdErr,
}

/// Print to stdout w timestamp
fn print(message: &str) {
   tp(message, Fd::StdOut);
}

/// Print to stderr w timestamp
fn eprint(message: &str) {
    tp(message, Fd::StdErr);
}

/// Print with timestamp
fn tp(message: &str, fd: Fd) {
    let local: DateTime<Local> = Local::now();
    match fd {
        Fd::StdOut => {
            println!("{}: {}", local.to_string(), message);
        },
        Fd::StdErr => {
            eprintln!("{}: {}", local.to_string(), message);
        }
    }
}

/// Execute command in a subprocess using Gnu Parallel with given input
/// Runs parallel instances of command with one item of input per instance
/// E.g. input ['a','b','c'] -> parallel-exec [command 'a', command 'b', command 'c']
//fn parallelize(command: &str, job_slots: &str, input: Vec<String>) -> Output {
fn parallelize(command: &str, job_slots: &str, input: Vec<String>)
   -> Result<(), Box<dyn Error>> {

    let mut parallel_args: Vec<String> = vec![];
    if job_slots != "100%" {
        parallel_args.push(format!("-j{}", job_slots));
    }
    parallel_args.push(command.to_owned());

    debug!("parallelizing with {} job slots", job_slots);

    let mut child = Command::new("parallel")
        .args(parallel_args)
        .stdin(Stdio::piped())
        //.stdout(Stdio::piped())
        .spawn()?;
        //.expect("failed to execute");

    let mut stdin = child.stdin.take().ok_or("Failed to open stdin")?;
    std::thread::spawn(move || {
        stdin.write_all(input.join("\n").as_bytes()).expect("Failed to write to stdin");
    });

    //return child.wait_with_output().expect("Failed to read stdout");
    child.wait()?;

    Ok(())
}

/// Fill a vector with all the file paths under the given dir (recursive)
/// that have the given extensions
/// TODO: Would be better to pass a closure for adding files to the list
/// Instead of checking if extensions is empty every iteration let the caller decide what determines
/// if a file gets appended to the list. It could pass one closure that unconditionally adds files if
/// the list of extensions is empty, otherwise a closure that checks the file against the extensions list
fn get_files(dir: &Path, extensions: &Vec<&str>, files: &mut Vec<String>) -> io::Result<()> {
    if dir.is_dir() {
        for e in fs::read_dir(dir)? {
            let entry = e?;
            let path = entry.path();
            if path.is_dir() {
                debug!("D {:?}", path);
                get_files(&path, extensions, files)?;
            } else if extensions.is_empty() {
                debug!("f {:?}", path);
                files.push(path.display().to_string());
            } else if
              path.extension().is_some() &&
              extensions.contains(&path.extension().unwrap()
                                       .to_str().unwrap()) {
                debug!("f {:?}", path);
                files.push(path.display().to_string());
            }
        }
    }
    Ok(())
}

fn should_term(term: &std::sync::Arc<std::sync::atomic::AtomicBool>) -> bool {
    if term.load(Ordering::Relaxed) {
        print(format!("PFP: CAUGHT SIGNAL! K Thx Bye!").as_str()); 
        return true;
    }
    false
}

fn get_chunk(index: usize, chunk_size: usize, files: &Vec<String>) -> Vec<String>{
    let start = index*chunk_size;
    let end = start+chunk_size;
    files[start..end].to_vec()
}

/// Do the thing forever unless interrupted.
/// Read all files in the input path and feed them in chunks to an invocation of Gnu Parallel
/// Wait for each chunk to complete before processing the next chunk
/// TODO: Possibly sleep after processing each chunk?
fn run(chunk_size: usize,
       job_slots: String,
       sleep_time: u64,
       daemon: bool,
       extensions: Vec<&str>,
       input_path: String,
       script: Option<String>)
   -> Result<(),Box<dyn Error>> {

    let term = std::sync::Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, std::sync::Arc::clone(&term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, std::sync::Arc::clone(&term))?;

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
        if ! files.is_empty() {
            panic!("files is not empty");
        }
        get_files(Path::new(&input_path), &extensions, &mut files)?;

        // 2. process chunks of input in parallel
        let num_chunks = files.len() / chunk_size;
        let leftover   = files.len() % chunk_size;
        debug!("number of chunks {}", num_chunks);
        debug!("leftover {}", leftover);

        // ??? how to break input up into chunks

        // Each chunk is a reference to slice of our Vec<String>:  &[String] &["foo", "bar",...,"baz"]
        // chunk 1:  [0..50] 0-49 50-things
        // chunk 2:  [50..100] 50-99 50-things
        // chunk 3:  [100..101] 100 1-thing

        for n in 0..num_chunks {
            debug!("chunk {} ({}): START", n+1, chunk_size);
            debug!("chunk start: {} chunk_end: {}", n*chunk_size, n*chunk_size+chunk_size-1);
            let chunk = get_chunk(n, chunk_size, &files);
            parallelize(&command, &slots, chunk)?;
            debug!("chunk {} ({}): DONE", n+1, chunk_size);
            if should_term(&term) {
                return Ok(());
            }
        }

        // last chunk
        if leftover != 0 {
            debug!("chunk {} ({}): START", num_chunks+1, leftover);
            debug!("chunk start: {} chunk_end: {}", num_chunks*chunk_size, num_chunks*chunk_size+leftover-1);
            let chunk = get_chunk(num_chunks, leftover, &files);
            parallelize(&command, &slots, chunk)?;
            debug!("chunk {} ({}): DONE", num_chunks+1, leftover);
        }

        // 3. Do any necessary postprocessing

        if (! daemon) || should_term(&term) {
            return Ok(());
        }

        print(format!("PFP: Finished processing all files in input-path. Sleeping for {} seconds...", sleep_time).as_str());
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

    env_logger::builder().target(env_logger::Target::Stdout).init();

    debug!("{:?}", opt);
    debug!("job_slots = {}", job_slots);

    if let Err(e) = run(opt.chunk_size,
                        job_slots,
                        opt.sleep_time,
                        opt.daemon,
                        ext_vec,
                        opt.input_path,
                        opt.script) {
        eprint(format!("Oh noes! {}", e).as_str());
        process::exit(1);
    }
}
