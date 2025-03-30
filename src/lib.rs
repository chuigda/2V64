pub mod socket;

use std::pin::Pin;
use std::future::Future;
use std::sync::{mpsc, Arc, Mutex};
use std::task::{Context, RawWaker, RawWakerVTable, Waker};

pub type TaskFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub struct Slava {
    scheduled: mpsc::Receiver<SlavaTask>,
    sender: mpsc::Sender<SlavaTask>,
}

impl Slava {
    pub fn slava() -> Arc<Self> {
        let (sender, scheduled) = mpsc::channel();
        Arc::new(Self { scheduled, sender })
    }

    pub fn spawn(&self, task_fut: impl Future<Output = ()> + Send + 'static) {
        let task = SlavaTask::new(self.sender.clone(), Box::pin(task_fut));
        self.sender.send(task).unwrap();
    }

    pub fn run(&self) {
        while let Ok(task) = self.scheduled.recv() {
            let waker = task.make_waker();
            let mut cx = Context::from_waker(&waker);
            let _ = task.task_fut.lock().unwrap().as_mut().poll(&mut cx);
        }
    }
}

// this is safe as far as we only call `run` in one thread... I guess
unsafe impl Sync for Slava {}

#[derive(Clone)]
struct SlavaTask {
    sender: mpsc::Sender<SlavaTask>,
    task_fut: Arc<Mutex<TaskFuture>>
}

impl SlavaTask {
    pub fn new(sender: mpsc::Sender<SlavaTask>, task_fut: TaskFuture) -> Self {
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
