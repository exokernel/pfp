use anyhow::Result;
use signal_hook::consts::{SIGINT, SIGTERM};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct ProcessingContext<'a> {
    pub chunk_size: usize,
    pub extensions: &'a Option<Vec<&'a OsStr>>,
    pub input_path: &'a Path,
    pub script: Option<&'a Path>,
    pub daemon: bool,
    pub sleep_time: u64,
    pub job_slots: Option<usize>,
    pub term: Arc<AtomicBool>,
}

impl<'a> ProcessingContext<'a> {
    pub fn setup_signal_handling(&self) -> Result<()> {
        signal_hook::flag::register(SIGTERM, self.term.clone())?;
        signal_hook::flag::register(SIGINT, self.term.clone())?;
        Ok(())
    }

    pub fn term_signal_rcvd(&self) -> bool {
        self.term.load(Ordering::Relaxed)
    }

    pub fn configure_thread_pool(&self) -> Result<()> {
        if let Some(slots) = self.job_slots {
            rayon::ThreadPoolBuilder::new()
                .num_threads(slots)
                .build_global()?;
        } else {
            rayon::ThreadPoolBuilder::new().build_global()?;
        }
        Ok(())
    }
}
