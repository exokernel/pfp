use std::io::{Write};
use std::io;
use std::process::{Command, Stdio, Output};
use std::process;
use std::error::Error;
use structopt::StructOpt;
use log::{debug};
use std::fs;
use std::path::{Path};

#[derive(Debug, StructOpt)]
#[structopt(name = "parallel-test", about = "Do some things in parallel")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Activate test mode
    #[structopt(short, long)]
    test: bool,

    /// Process files in input path continuously
    #[structopt(long)]
    daemon: bool,

    /// Number of things to try to do in parallel at one time.
    /// This is the number inputs that will be fed to a single invocation of
    /// Gnu Parallel. The actual number of parallel jobs per chunk is limited
    /// by job_slots.
    #[structopt(short, long, default_value = "50")]
    chunk_size: usize,

    /// Number of parallel job slots to use. Default to 1 slot per CPU core.
    #[structopt(short, long)]
    job_slots: Option<usize>,

    /// Shell command or script to run in parallel
    #[structopt(short, long)]
    script: Option<String>,

    /// Directory to read files from
    input_path: String,
}

/// Execute command in a subprocess using Gnu Parallel with given input
/// Runs parallel instances of command with one item of input per instance
/// E.g. input ['a','b','c'] -> parallel-exec [command 'a', command 'b', command 'c']
fn parallelize(command: &str, job_slots: &str, input: Vec<String>) -> Output {

    let mut parallel_args: Vec<String> = vec![];
    if job_slots != "100%" {
        parallel_args.push(format!("-j{}", job_slots));
    }
    parallel_args.push(command.to_owned());

    debug!("parallelizing with {} job slots", job_slots);

    let mut child = Command::new("parallel")
        .args(parallel_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin.write_all(input.join("\n").as_bytes()).expect("Failed to write to stdin");
    });

    return child.wait_with_output().expect("Failed to read stdout");
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
                get_files(&path, extensions, files).unwrap();
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

/// Do the thing forever unless interrupted.
/// Read all files in the input path and feed them in chunks to an invocation of Gnu Parallel
/// Wait for each chunk to complete before processing the next chunk
/// TODO: Possibly sleep after processing each chunk?
fn run(chunk_size: usize,
       job_slots: String,
       sleep_time: f64,
       daemon: bool,
       input_path: String,
       script: Option<String>)
   -> Result<(),Box<dyn Error>> {

    let slots = job_slots.as_str();

    let mut command = String::from("echo {#}-{%}-{}");
    if script.is_some() {
        command = script.unwrap();
    }

    //let extensions = vec!["mp4", "flv"];
    let extensions = vec![];

    // Do forever
    loop {

        // 1. Get all the files in our input path
        let mut files: Vec<String> = vec![];
        get_files(Path::new(&input_path), &extensions, &mut files).unwrap();
        files.sort();

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

        for i in 0..num_chunks {
            println!("chunk {}", i + 1);
            let index = chunk_size*i;
            let chunk = (&files[index..(index+chunk_size)]).to_vec();
            let output = parallelize(&command, slots, chunk);
            println!("{}",  String::from_utf8_lossy(&output.stdout));
        }
        // last chunk
        if leftover != 0 {
            println!("chunk {}", num_chunks + 1);
            let index = chunk_size*num_chunks;
            let chunk = (&files[index..(index+leftover)]).to_vec();
            let output = parallelize(&command, slots, chunk);
            println!("{}",  String::from_utf8_lossy(&output.stdout));
        }

        // 3. Do any necessary postprocessing

        if ! daemon {
            return Ok(());
        }

        // TODO: sleep before looking in a path again
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

    env_logger::init();

    debug!("{:?}", opt);
    debug!("job_slots = {}", job_slots);

    if let Err(e) = run(opt.chunk_size,
                        job_slots, 0 as f64,
                        opt.daemon,
                        opt.input_path,
                        opt.script) {
        eprintln!("Oh noes! {}", e);
        process::exit(1);
    }
}
