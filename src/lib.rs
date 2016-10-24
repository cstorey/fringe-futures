extern crate fringe;
extern crate futures;
#[macro_use]
extern crate log;

use std::panic;
use fringe::OsStack;
use fringe::generator::{Generator, Yielder};
use futures::{Future, Poll, Async};

#[derive(Debug, PartialEq, Eq)]
enum WaitCommand {
    Continue,
    Terminate,
}

#[derive(Debug)]
pub struct FringeFut<T: Send, E: Send> {
    gen: Generator<WaitCommand, Async<Result<T, E>>, OsStack>,
}

#[derive(Debug, Clone)]
pub struct SchedThunk<'a, T: Send + 'a, E: Send + 'a>(&'a Yielder<WaitCommand,
                                                                  Async<Result<T, E>>>);

const STACKSZ: usize = 1 << 20;

impl<T: Send, E: Send> FringeFut<T, E> {
    pub fn new<F>(f: F) -> Self
        where F: FnOnce(SchedThunk<T, E>) -> Result<T, E> + Send
    {
        let stack = OsStack::new(STACKSZ).expect("OsStack::new");
        let gen = {
            Generator::new(stack, move |yielder, mut _in| {
                assert_eq!(_in, WaitCommand::Continue);

                let wr = SchedThunk(yielder);
                let res = f(wr);
                yielder.suspend(Async::Ready(res));
            })
        };
        FringeFut { gen: gen }
    }
}

impl<'a, T: Send + 'a, E: Send + 'a> SchedThunk<'a, T, E> {
    pub fn await<F: Future>(&self, mut f: F) -> Result<F::Item, F::Error> {
        loop {
            let cmd = match f.poll() {
                Ok(Async::NotReady) => self.0.suspend(Async::NotReady),
                Ok(Async::Ready(val)) => return Ok(val),
                Err(e) => return Err(e),
            };

            match cmd {
                WaitCommand::Continue => (),
                WaitCommand::Terminate => panic!("FringeFut exiting"),
            }
        }
    }
}

impl<T: Send, E: Send> Future for FringeFut<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.gen.resume(WaitCommand::Continue) {
            Some(Async::NotReady) => Ok(Async::NotReady),
            Some(Async::Ready(Ok(r))) => Ok(Async::Ready(r)),
            Some(Async::Ready(Err(e))) => Err(e),
            None => {
                panic!("Future generator exited");
            }
        }
    }
}

impl<T: Send, E: Send> Drop for FringeFut<T, E> {
    fn drop(&mut self) {
        let result =
            panic::catch_unwind(panic::AssertUnwindSafe(|| self.gen.resume(WaitCommand::Terminate)));
        match result {
            Ok(None) => {}
            Ok(Some(_)) => {
                warn!("FringeFut continued after termination; leaking owned resources");
            }
            Err(e) => {
                warn!("Dropped unterminated thread:{:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod test {
    extern crate env_logger;
    use std::mem;
    use std::sync::{Arc, atomic};
    use futures::{self, Future, Poll, Async, task};
    use super::FringeFut;
    #[derive(Debug)]
    enum CanaryFuture {
        Ready(usize),
        Go(usize),
        Done,
    }

    impl Future for CanaryFuture {
        type Item = usize;
        type Error = ();
        fn poll(&mut self) -> Poll<usize, ()> {
            println!("CanaryFuture#poll:{:?}", self);
            match mem::replace(self, CanaryFuture::Done) {
                CanaryFuture::Ready(n) => {
                    *self = CanaryFuture::Go(n);
                    task::park().unpark();
                    Ok(Async::NotReady)
                }
                CanaryFuture::Go(n) => Ok(Async::Ready(n)),
                CanaryFuture::Done => unreachable!(),
            }
        }
    }

    #[test]
    fn smoketest() {
        env_logger::init().unwrap_or(());
        let f = FringeFut::<usize, ()>::new(|yielder| {
            for n in 0..5 {
                let r = yielder.await(CanaryFuture::Ready(n)).expect("await");
                println!("a:{:?}", r);
            }
            Ok(42usize)
        });

        println!("Before:{:?}", f);
        // Helpfully seems to run things inside of a task for us.
        let res = f.wait();
        println!("res:{:?}", res);
    }

    #[test]
    fn should_drop_resources_on_cancel() {
        #[derive(Debug)]
        struct MyDroppable(Arc<atomic::AtomicBool>);
        impl Drop for MyDroppable {
            fn drop(&mut self) {
                println!("Dropping!");
                self.0.store(true, atomic::Ordering::SeqCst);
            }
        }

        env_logger::init().unwrap_or(());

        let canary = Arc::new(atomic::AtomicBool::new(false));

        let mut f = FringeFut::<(), ()>::new(|yielder| {
            let myresource = MyDroppable(canary.clone());
            println!("Starting with:{:?}", myresource);
            yielder.await(futures::empty::<(), ()>()).expect("await");
            Ok(())
        });

        assert_eq!(f.poll(), Ok(Async::NotReady));
        drop(f);
        println!("Resource dropped? {:?}", canary);
        assert_eq!(canary.load(atomic::Ordering::SeqCst), true)
    }
}
