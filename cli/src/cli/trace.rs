use super::progress::{Event, report};
use std::{
    sync::{Mutex, OnceLock},
    thread::ThreadId,
};

macro_rules! report_span {
    ($($arg:tt)*) => {
        let _guard = $crate::cli::StatusReporter::get().request_span(format!($($arg)*));
    };
}

macro_rules! report_message {
    ($($arg:tt)*) => {
        $crate::cli::StatusReporter::get().report_message(format!($($arg)*));
    };
}

pub(crate) use {report_message, report_span};

pub struct StatusTrace {
    pub span: String,
    pub message: String,
}

#[doc(hidden)]
pub struct StatusReporter {
    stacks: Mutex<Vec<(ThreadId, Vec<StatusTrace>)>>,
}

impl StatusReporter {
    pub fn get() -> &'static Self {
        static INSTANCE: OnceLock<StatusReporter> = OnceLock::new();
        INSTANCE.get_or_init(|| Self {
            stacks: Mutex::new(Vec::new()),
        })
    }

    pub fn request_span(&self, span: String) -> impl Drop {
        struct EndStatus(ThreadId);
        impl Drop for EndStatus {
            fn drop(&mut self) {
                StatusReporter::get().report_end(self.0);
            }
        }

        let thread = std::thread::current().id();
        StatusReporter::get().report_start(thread, span);
        EndStatus(thread)
    }

    pub fn report_message(&self, message: String) {
        let thread = std::thread::current().id();
        self.stacks
            .lock()
            .unwrap()
            .iter_mut()
            .find(|x| x.0 == thread)
            .map(|(_, stack)| stack.last_mut().map(|x| x.message = message.clone()));

        report(Event::Message(message));
    }

    pub fn report_start(&self, thread: ThreadId, span: String) {
        let trace = StatusTrace {
            span: span.clone(),
            message: span.clone(),
        };

        let mut stacks = self.stacks.lock().unwrap();
        match stacks.iter_mut().find(|x| x.0 == thread) {
            Some((_, stack)) => stack.push(trace),
            None => stacks.push((thread, vec![trace])),
        }

        drop(stacks);
        report(Event::Begin(span));
    }

    pub fn report_end(&self, thread: ThreadId) -> bool {
        let mut stacks = self.stacks.lock().unwrap();
        match stacks.iter_mut().find(|x| x.0 == thread) {
            Some((_, stack)) => {
                stack.pop();
                if stack.is_empty() {
                    stacks.retain(|x| x.0 != thread);
                }
            }
            None => {
                return false;
            }
        }

        drop(stacks);
        report(Event::End);
        true
    }

    pub fn request_trace<T>(
        &self,
        thread: ThreadId,
        cb: impl FnOnce(&[StatusTrace]) -> T,
    ) -> Option<T> {
        self.stacks
            .lock()
            .unwrap()
            .iter()
            .find(|x| x.0 == thread)
            .map(|(_, stack)| cb(stack))
    }

    pub fn request_trace_all(&self, mut cb: impl FnMut(ThreadId, &[StatusTrace])) {
        let stacks = self.stacks.lock().unwrap();
        for (thread, stack) in stacks.iter() {
            cb(*thread, stack);
        }
    }
}
