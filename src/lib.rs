pub mod socket;
pub mod bufread;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, RawWaker, RawWakerVTable, Waker};
use std::thread::spawn as thread_spawn;

use crossbeam::channel::{unbounded as channel_unbounded, Sender, Receiver};

pub type TaskFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub struct Slava {
    scheduled: Receiver<SlavaTask>,
    sender: Sender<SlavaTask>,
}

impl Slava {
    pub fn slava() -> Arc<Self> {
        let (sender, scheduled) = channel_unbounded();
        Arc::new(Self { scheduled, sender })
    }

    pub fn spawn(&self, task_fut: impl Future<Output = ()> + Send + 'static) {
        let task = SlavaTask::new(self.sender.clone(), Box::pin(task_fut));
        self.sender.send(task).unwrap();
    }

    pub fn run(&self, n_worker_thread: usize) {
        let mut join_handles = Vec::new();

        for _ in 0..n_worker_thread {
            let scheduled = self.scheduled.clone();
            join_handles.push(thread_spawn(move || {
                while let Ok(task) = scheduled.recv() {
                    let waker = task.make_waker();
                    let mut cx = Context::from_waker(&waker);
                    let _ = task.task_fut.lock().unwrap().as_mut().poll(&mut cx);
                }
            }));
        }

        for handle in join_handles {
            let _ = handle.join();
        }
    }

    pub fn run_singlethreaded(&self) {
        while let Ok(task) = self.scheduled.recv() {
            let waker = task.make_waker();
            let mut cx = Context::from_waker(&waker);
            let _ = task.task_fut.lock().unwrap().as_mut().poll(&mut cx);
        }
    }
}

#[derive(Clone)]
struct SlavaTask {
    sender: Sender<SlavaTask>,
    task_fut: Arc<Mutex<TaskFuture>>
}

impl SlavaTask {
    pub fn new(sender: Sender<SlavaTask>, task_fut: TaskFuture) -> Self {
        Self {
            sender,
            task_fut: Arc::new(Mutex::new(task_fut))
        }
    }

    pub fn make_waker(&self) -> Waker {
        unsafe {
            Waker::from_raw(RawWaker::new(
                Box::into_raw(Box::new(self.clone())) as *const (),
                &SLAVA_WAKER_VTABLE
            ))
        }
    }

    unsafe fn clone_raw(data: *const ()) -> RawWaker {
        let waker = unsafe { &*(data as *const SlavaTask) };
        RawWaker::new(
            Box::into_raw(Box::new(waker.clone())) as *const (),
            &SLAVA_WAKER_VTABLE
        )
    }

    unsafe fn wake_raw(data: *const ()) {
        let waker = unsafe { &*(data as *const SlavaTask) };
        waker.sender.send(waker.clone()).unwrap();
    }

    unsafe fn wake_by_ref_raw(data: *const ()) {
        let waker = unsafe { &*(data as *const SlavaTask) };
        waker.sender.send(waker.clone()).unwrap();
    }

    unsafe fn drop_raw(data: *const ()) {
        drop(unsafe { Box::from_raw(data as *mut SlavaTask) });
    }
}

pub const SLAVA_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    SlavaTask::clone_raw,
    SlavaTask::wake_raw,
    SlavaTask::wake_by_ref_raw,
    SlavaTask::drop_raw
);
