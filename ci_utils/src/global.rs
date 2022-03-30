use crate::prelude::*;

use crate::future::try_join_all;
use crate::future::AsyncPolicy;
use indicatif::ProgressBar;
use std::lazy::SyncLazy;
use std::sync::Mutex;
use std::sync::Weak;
use std::time::Duration;
use tokio::task::JoinHandle;

const REFRESHES_PER_SECOND: u32 = 50;

#[derive(Debug)]
struct GlobalState {
    bars:          Vec<Weak<ProgressBar>>,
    _tick_thread:  std::thread::JoinHandle<()>,
    ongoing_tasks: Vec<JoinHandle<Result>>,
}

impl GlobalState {
    pub fn tick(&mut self) {
        let mut to_remove = vec![];
        for (index, bar) in self.bars.iter().enumerate() {
            if let Some(bar) = bar.upgrade() {
                bar.tick()
            } else {
                to_remove.push(index)
            }
        }

        for to_remove in to_remove.iter().rev() {
            self.bars.remove(*to_remove);
        }
    }
}

impl Default for GlobalState {
    fn default() -> Self {
        GlobalState {
            bars:          default(),
            _tick_thread:  std::thread::spawn(|| {
                GLOBAL.lock().unwrap().tick();
                std::thread::sleep(Duration::SECOND / REFRESHES_PER_SECOND);
            }),
            ongoing_tasks: default(),
        }
    }
}

static GLOBAL: SyncLazy<Mutex<GlobalState>> = SyncLazy::new(default);

pub fn progress_bar(f: impl FnOnce() -> ProgressBar) -> Arc<ProgressBar> {
    let ret = Arc::new(f());
    GLOBAL.lock().unwrap().bars.push(Arc::downgrade(&ret));
    ret
}

pub fn new_spinner(message: impl Into<Cow<'static, str>>) -> Arc<ProgressBar> {
    let ret = progress_bar(indicatif::ProgressBar::new_spinner);
    ret.set_message(message);
    ret
}

pub fn spawn(name: impl AsRef<str>, f: impl Future<Output = Result> + Send + 'static) {
    info!("Spawning a new global task named '{}'.", name.as_ref());
    let join_handle = tokio::task::Builder::new().name(name.as_ref()).spawn(f);
    GLOBAL.lock().unwrap().ongoing_tasks.push(join_handle);
}


pub async fn complete_tasks() -> Result {
    info!("Waiting for remaining tasks to complete.");
    while let tasks = std::mem::replace(&mut GLOBAL.lock().unwrap().ongoing_tasks, default()) && !tasks.is_empty() {
        info!("Found {} tasks to wait upon.", tasks.len());
        try_join_all(tasks, AsyncPolicy::FutureParallelism).await?;
    }
    info!("All pending tasks have been completed.");
    Ok(())
}


//
// pub fn complete_tasks(rt: &Runtime) -> Result {
//     info!("Waiting for remaining tasks to complete.");
//     while let tasks = std::mem::replace(&mut GLOBAL.lock().unwrap().ongoing_tasks, default()) &&
// !tasks.is_empty() {         let tasks = try_join_all(tasks, AsyncPolicy::FutureParallelism);
//          rt.block_on(tasks)?;
//     }
//     Ok(())
// }
