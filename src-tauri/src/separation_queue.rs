use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;

pub(crate) struct SeparationTask {
    pub app: AppHandle,
    pub song_id: String,
    pub job_token: String,
    pub input_path: String,
    pub output_dir: PathBuf,
    pub song_duration_ms: u64,
    pub model_id: String,
}

static QUEUE: Mutex<VecDeque<SeparationTask>> = Mutex::new(VecDeque::new());
static WORKER_RUNNING: Mutex<bool> = Mutex::new(false);

pub(crate) fn submit_task(task: SeparationTask) {
    {
        let mut queue = QUEUE.lock().unwrap();
        queue.push_back(task);
    }
    try_start_worker();
}

pub(crate) fn cancel_task(song_id: &str) -> bool {
    let mut queue = QUEUE.lock().unwrap();
    if let Some(pos) = queue.iter().position(|t| t.song_id == song_id) {
        queue.remove(pos);
        true
    } else {
        false
    }
}

pub(crate) fn is_queued(song_id: &str) -> bool {
    let queue = QUEUE.lock().unwrap();
    queue.iter().any(|t| t.song_id == song_id)
}

fn try_start_worker() {
    let mut running = WORKER_RUNNING.lock().unwrap();
    if !*running {
        *running = true;
        std::thread::spawn(worker_loop);
    }
}

fn worker_loop() {
    loop {
        let task = {
            let mut queue = QUEUE.lock().unwrap();
            queue.pop_front()
        };

        match task {
            Some(t) => {
                crate::update_song_status(&t.song_id, "processing", 0, Some("checking_gpu"), None);
                crate::process_song_background(
                    t.app,
                    t.song_id,
                    t.job_token,
                    t.input_path,
                    t.output_dir,
                    t.song_duration_ms,
                    false,
                    t.model_id,
                );
            }
            None => {
                let mut running = WORKER_RUNNING.lock().unwrap();
                *running = false;
                break;
            }
        }
    }
}
