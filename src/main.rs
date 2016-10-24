extern crate fringe;
extern crate futures;
extern crate tokio_timer;
extern crate futures_cpupool;
use fringe::OsStack;
use fringe::generator::{Generator, Yielder};
use futures::{Future, Poll, Async};
use std::rc::Rc;
use std::cell::RefCell;
use tokio_timer::*;
use std::time::Duration;
use futures_cpupool::CpuPool;

struct FringeFut<T: Send, E: Send> {
    gen: Generator<(), Async<Result<T, E>>, OsStack>,
}

struct SchedThunk<'a, T: Send + 'a, E: Send + 'a>(&'a Yielder<(), Async<Result<T, E>>>);
const STACKSZ  : usize = 1<<20;

// impl<'a, Input, Output, Stack> Generator<'a, Input, Output, Stack> where Input: 'a, Output: 'a, Stack: Stack
// fn resume(&mut self, input: Input) -> Option<Output>
impl<T: Send, E: Send> FringeFut<T, E> {
    fn new<F>(f: F) -> Self
        where F: FnOnce(SchedThunk<T, E>) -> Result<T, E> + Send
    {
        let stack = OsStack::new(STACKSZ).expect("OsStack::new");
        let mut gen = {
            Generator::new(stack, move |yielder, mut _in: ()| {
                let wr = SchedThunk(yielder);
                let res = f(wr);
                yielder.suspend(Async::Ready(res));
            })
        };
        FringeFut { gen: gen }
    }
}
impl<'a, T: Send + 'a, E: Send + 'a> SchedThunk<'a, T, E> {
    fn await<F: Future>(&self, mut f: F) -> Result<F::Item, F::Error> {
        loop {
            match f.poll() {
                Ok(Async::NotReady) => self.0.suspend(Async::NotReady),
                Ok(Async::Ready(val)) => return Ok(val),
                Err(e) => return Err(e),
            }
        }
    }
}

impl<T: Send, E: Send> Future for FringeFut<T, E> {
    type Item = T;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.gen.resume(()) {
            Some(Async::NotReady) => Ok(Async::NotReady),
            Some(Async::Ready(Ok(r))) => Ok(Async::Ready(r)),
            Some(Async::Ready(Err(e))) => Err(e),
            None => {
                panic!("Future generator exited");
            }
        }
    }
}

fn main() {
    let pool = CpuPool::new(4);

    let timer = Timer::default();

    // Execute some work on the thread pool, optionally closing over data.
    let f = FringeFut::<usize, ()>::new(|yielder| {
        for n in 0..10 {
            let dur = Duration::from_millis(100) * n;
            println!("n:{:?}", dur);
            yielder.await(timer.sleep(dur));
        }
        Ok(42usize)
    });

    let res = f.wait();
    println!("res:{:?}", res);
}
