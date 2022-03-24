use crate::prelude::*;

use indicatif::MultiProgress;
use indicatif::ProgressBar;
use std::cell::RefCell;
use std::lazy::SyncLazy;
use std::sync::Mutex;
use std::sync::Weak;
use std::time::Duration;

const REFRESHES_PER_SECOND: u32 = 50;

#[derive(Debug)]
struct GlobalState {
    bars:         Vec<Weak<ProgressBar>>,
    _tick_thread: std::thread::JoinHandle<()>,
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
            bars:         default(),
            _tick_thread: std::thread::spawn(|| {
                GLOBAL.lock().unwrap().tick();
                std::thread::sleep(Duration::SECOND / REFRESHES_PER_SECOND);
            }),
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
    let mut ret = progress_bar(indicatif::ProgressBar::new_spinner);
    ret.set_message(message);
    ret
}
