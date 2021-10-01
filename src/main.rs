use std::io::{Write};
use std::io;
use std::process::{Command, Stdio, Output, ExitStatus};
use std::process;
use std::error::Error;
use structopt::StructOpt;
use log::{debug};
use std::fs::{self, DirEntry};
//use std::fs;
use std::path::{Path,PathBuf};

#[derive(Debug, StructOpt)]
#[structopt(name = "parallel-test", about = "Do some things in parallel")]
struct Opt {
    /// Activate debug mode
    #[structopt(short, long)]
    debug: bool,

    /// Activate test mode
    #[structopt(short, long)]
    test: bool,

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
/// Runs parallel instances of command with on item of input per instance
/// E.g. input ['a','b','c'] -> parallel-exec [command 'a', command 'b', command 'c']
fn parallelize(command: &str, job_slots: &str, input: Vec<String>) -> Output {
    debug!("parallelizing with {} job slots", job_slots);
    let mut child = Command::new("parallel")
        .arg(command)
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

fn visit_dirs(dir: &Path, file_list: &mut Vec<String>) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, file_list)?;
            } else {
                //cb(&entry);
                file_list.push(path.display().to_string());
            }
        }
    }
    Ok(())
}

fn print_entries(input_path: String) -> io::Result<()> {
    let mut entries = fs::read_dir(input_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    // The order in which `read_dir` returns entries is not guaranteed. If reproducible
    // ordering is required the entries should be explicitly sorted.

    entries.sort();
    for e in entries {
        println!("{}", e.display().to_string());
    }

    Ok(())
}

/// Return a vector of all the file paths in the given dir
/// TODO: process directories recursively building a list of files
fn get_files(dir: &Path, files: &mut Vec<String>) -> io::Result<()> {
    if dir.is_dir() {
        // print info about each dir ent
        for e in fs::read_dir(dir)? {
            let entry = e?;
            let path = entry.path();
            if path.is_dir() {
                debug!("D {:?}", path);
                get_files(&path, files).unwrap();
            } else {
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
fn run(chunk_size: usize, job_slots: String, sleep_time: f64, input_path: String)
   -> Result<(),Box<dyn Error>> {

    // Do forever
    loop {

        // Get all the files in our input path
        let mut files: Vec<String> = vec![];
        get_files(Path::new(&input_path), &mut files).unwrap();

        // 1. get a whole set of input (e.g. names of all files in some directory)
        //    if there's no input sleep for a while and try again

        // 2. process chunks of input in parallel
        let num_chunks = files.len() / chunk_size; // 2
        let leftover   = files.len() % chunk_size; // 1
        debug!("number of chunks {}", num_chunks);
        debug!("leftover {}", leftover);

        // ??? how to break input up into chunks

        // Each chunk is a reference to slice of our Vec<String>:  &[String] &["foo", "bar",...,"baz"]
        // chunk 1:  [0..50] 0-49 50-things
        // chunk 2:  [50..100] 50-99 50-things
        // chunk 3:  [100..101] 100 1-thing

        let slots = job_slots.as_str();

        for i in 0..num_chunks {
            println!("chunk {}", i + 1);
            let index = chunk_size*i;
            let chunk = (&files[index..(index+chunk_size)]).to_vec();
            let output = parallelize("echo {#}-{%}-{}", slots, chunk);
            println!("{}",  String::from_utf8_lossy(&output.stdout));
        }
        // last chunk
        if leftover != 0 {
            println!("chunk {}", num_chunks + 1);
            let index = chunk_size*num_chunks;
            let chunk = (&files[index..(index+leftover)]).to_vec();
            let output = parallelize("echo {#}-{%}-{}", slots, chunk);
            println!("{}",  String::from_utf8_lossy(&output.stdout));
        }

        return Ok(());
        // 3. Do any necessary postprocessing

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
    if ! opt.job_slots.is_none() {
        job_slots = opt.job_slots.unwrap().to_string();
    }

    env_logger::init();

    debug!("{:?}", opt);
    debug!("job_slots = {}", job_slots);

    /*
    //print_entries(opt.input_path).unwrap();
    let mut flist: Vec<String> = vec![];
    visit_dirs(Path::new(&opt.input_path), &mut flist).unwrap();

    flist.sort();
    for f in flist {
        println!("{}", f);
    }
    return ();
    */
    if let Err(e) = run(opt.chunk_size, job_slots, 0 as f64, opt.input_path) {
        eprintln!("Oh noes! {}", e);
        process::exit(1);
    }
}
